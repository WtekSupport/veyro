# Launch the release build from the configured Cargo target directory.

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "ensure-admin.ps1") -CallerScript $PSCommandPath -Wait

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")

$exe = Join-Path $env:CARGO_TARGET_DIR "release\veyro.exe"
if (-not (Test-Path $exe)) {
    $legacy = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")) "src-tauri\target\release\veyro.exe"
    if (Test-Path $legacy) { $exe = $legacy }
}
if (-not (Test-Path $exe)) {
    Write-Error "Release exe not found. Run: npm run tauri:build"
}

Write-Host "Starting $exe"
Start-Process -FilePath $exe
