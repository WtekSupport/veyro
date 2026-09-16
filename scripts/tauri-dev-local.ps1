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

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

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

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo not found in PATH. Install Rust: https://rustup.rs/"
}

Write-Host "Using cargo: $(Get-Command cargo | Select-Object -ExpandProperty Source)"
Write-Host "Build output: $env:CARGO_TARGET_DIR\"

. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
$parallel = Set-LlamaCppBuildParallelism

if ($features -match "local-llm") {
    & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Parallel $parallel
}

Write-Host "Selected features: $features"

$tauriJs = Join-Path (Join-Path (Join-Path $repoRoot "node_modules") "@tauri-apps\cli") "tauri.js"
if (-not (Test-Path $tauriJs)) {
    Write-Error "Tauri CLI not found. Run npm install in the repo root."
}

$nodeExe = (Get-Command node -ErrorAction Stop).Source
& $nodeExe $tauriJs dev --features $features -- -j $parallel
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
