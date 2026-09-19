# Dev mode with local Whisper (whisper-rs). Build artifacts: C:\veyro-target (Windows default).

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "ensure-admin.ps1") -CallerScript $PSCommandPath -Wait

if ($IsWindows -or $env:OS -like "*Windows*") {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Error "Veyro dev mode requires administrator privileges. Use: npm run tauri:dev"
        exit 1
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
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

Write-Host "Using cargo: $cargoExe"

$devLog = Join-Path $env:CARGO_TARGET_DIR ("tauri-dev-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))
try {
    Start-Transcript -Path $devLog | Out-Null
    Write-Host "Session log: $devLog"
} catch {
    Write-Warning "Could not start transcript log: $_"
}

function Stop-VeyroDevBuildProcesses {
    foreach ($procName in @("veyro", "cargo", "rustc", "msbuild")) {
        Get-Process -Name $procName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
}
Write-Host "Build output: $env:CARGO_TARGET_DIR\"

. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
$parallel = Set-LlamaCppBuildParallelism -RepoRoot $repoRoot

Write-Host "Selected features: $features"

$featureArgs = @("--no-default-features")
if ($features) {
    $featureArgs += @("--features", $features)
}

function Invoke-DevCargoBuild {
    Push-Location (Join-Path $repoRoot "src-tauri")
    try {
        & $cargoExe build @featureArgs -j $parallel
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
        Write-Host "llama.cpp already installed ($llamaDll) - skipping pre-dev cargo build"
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
            & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Profile "debug" -Parallel $parallel
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

$nodeExe = (Get-Command node -ErrorAction Stop).Source
& $nodeExe $tauriJs dev --features $features -- -j $parallel
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
