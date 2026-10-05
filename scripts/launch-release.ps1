# Launch the release build from the configured Cargo target directory.

$ErrorActionPreference = "Stop"

if (($IsWindows -eq $true) -or ($env:OS -like "*Windows*")) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        . (Join-Path $PSScriptRoot "ensure-admin.ps1") -CallerScript $PSCommandPath
        exit 1
    }
}

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
