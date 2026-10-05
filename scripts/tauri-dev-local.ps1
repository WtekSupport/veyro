# Dev mode with local Whisper (whisper-rs). Build artifacts: C:\veyro-target (Windows default).

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

# Dev must run elevated so injected text reaches apps running as administrator.
function Test-VeyroDevIsAdmin {
    if (-not (($IsWindows -eq $true) -or ($env:OS -like "*Windows*"))) {
        return $true
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-VeyroDevIsAdmin)) {
    if ($env:VEYRO_DEV_NO_ELEVATE -eq "1") {
        Write-Warning "VEYRO_DEV_NO_ELEVATE=1: skipping UAC relaunch; cargo run will fail with os error 740 unless you start from an elevated shell."
    } else {
        # Dot-source so `exit` in ensure-admin stops this instance after the elevated child exits.
        . (Join-Path $PSScriptRoot "ensure-admin.ps1") -CallerScript $PSCommandPath
        if (-not (Test-VeyroDevIsAdmin)) {
            Write-Error @"
Could not run Veyro dev elevated (UAC denied or elevation failed).
Accept the UAC prompt (check the taskbar / other desktop), open PowerShell as Administrator and run: npm run tauri:dev
Or run without elevation: `$env:VEYRO_DEV_NO_ELEVATE='1'; npm run tauri:dev
Or: npm run tauri:dev:direct
"@
            exit 1
        }
    }
}

Set-Location -LiteralPath $repoRoot

& node (Join-Path $PSScriptRoot "bump-version.mjs") dev
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

# Minimal shells may miss System32 and Node.js on PATH.
$system32 = Join-Path $env:SystemRoot "System32"
$nodejsDir = Join-Path ${env:ProgramFiles} "nodejs"
$pathPrefix = @($system32, $nodejsDir) | Where-Object { Test-Path $_ }
if ($pathPrefix.Count -gt 0) {
    $env:PATH = ($pathPrefix -join ";") + ";$env:PATH"
}

& (Join-Path $PSScriptRoot "ensure-cmake.ps1")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")

$cmakeBin = Join-Path (Join-Path (Join-Path $repoRoot ".tools") "cmake") "bin"
if (Test-Path (Join-Path $cmakeBin "cmake.exe")) {
    $env:PATH = "$cmakeBin;$env:PATH"
}

& (Join-Path $PSScriptRoot "ensure-rust-path.ps1")

$cargoExe = Get-Command cargo -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (-not $cargoExe) {
    Write-Error "cargo not found in PATH. Install Rust: https://rustup.rs/"
}

$devLog = Join-Path $env:CARGO_TARGET_DIR ("tauri-dev-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))
try {
    Start-Transcript -Path $devLog | Out-Null
} catch {
    Write-Warning "Could not start transcript log: $_"
}

function Stop-VeyroDevBuildProcesses {
    foreach ($procName in @("veyro", "cargo", "rustc", "msbuild")) {
        Get-Process -Name $procName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
}
. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
$cargoJobs = Set-LlamaCppBuildParallelism -RepoRoot $repoRoot
$cmakeParallel = Get-CmakeBuildParallelism

if ($null -eq $env:VEYRO_CARGO_INCREMENTAL -and ($IsWindows -or $env:OS -like "*Windows*")) {
    $env:CARGO_INCREMENTAL = "0"
}

Write-Host "Dev: target=$($env:CARGO_TARGET_DIR) | cargo=$cargoExe | features=$features | log=$devLog"

$featureArgs = Get-CargoFeatureArgs -Features $features
Enable-DiarizationBlasLink -RepoRoot $repoRoot -Features $features

function Invoke-DevCargoBuild {
    Push-Location (Join-Path $repoRoot "src-tauri")
    try {
        & $cargoExe build @featureArgs -j $cargoJobs
        return $LASTEXITCODE
    } finally {
        Pop-Location
    }
}

if ($features -match "local-llm") {
    $llamaDll = Get-ChildItem (Join-Path $env:CARGO_TARGET_DIR "debug\build") -Directory -Filter "llama-cpp-sys-2-*" -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName "out\bin\llama.dll" } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
    if ($llamaDll) {
        Write-Host "llama.cpp: cached (skip pre-dev cargo build)"
    } else {
        Write-Host "Pre-building debug binary (llama.cpp first build can take 30-40+ min)..."
        Stop-VeyroDevBuildProcesses
        $buildExit = 1
        for ($attempt = 1; $attempt -le 3; $attempt++) {
            if ($attempt -gt 1) {
                Write-Warning "Debug cargo build retry $attempt/3..."
            }
            $buildExit = Invoke-DevCargoBuild
            if ($buildExit -eq 0) {
                break
            }
            Write-Warning "cargo build failed ($buildExit) - running finish-llama-cpp-build..."
            & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Profile "debug" -Parallel $cmakeParallel
            $llamaDll = Get-ChildItem (Join-Path $env:CARGO_TARGET_DIR "debug\build") -Directory -Filter "llama-cpp-sys-2-*" -ErrorAction SilentlyContinue |
                ForEach-Object { Join-Path $_.FullName "out\bin\llama.dll" } |
                Where-Object { Test-Path $_ } |
                Select-Object -First 1
            if ($llamaDll) {
                Write-Host "llama.dll present after finish ($llamaDll)"
            }
        }
        if ($buildExit -ne 0) {
            Write-Error @"
Debug cargo build failed ($buildExit) after 3 attempts.
Warnings about git/LICENSE/OpenSSL in llama.cpp are normal for crates.io builds.
If vulkan-shaders-gen keeps failing, try again or set VEYRO_DISABLE_LOCAL_LLM=1 for a cloud-only dev session.
"@
        }
    }
}

$tauriJs = Join-Path (Join-Path (Join-Path $repoRoot "node_modules") "@tauri-apps\cli") "tauri.js"
if (-not (Test-Path $tauriJs)) {
    Write-Error "Tauri CLI not found. Run npm install in the repo root."
}

if ($features -match "silero-te") {
    & (Join-Path $PSScriptRoot "stage-libtorch-dlls.ps1") -RepoRoot $repoRoot -Profile "debug"
}

$nodeExe = (Get-Command node -ErrorAction Stop).Source
$devConfig = Join-Path $repoRoot "src-tauri/tauri.dev.conf.json"
& $nodeExe $tauriJs dev --config $devConfig --features $features -- -j $cargoJobs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
