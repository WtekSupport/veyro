# Sign NSIS setup.exe for the current version (after bundle, or if tauri build stopped at password prompt).
param(
    [string]$Semver = ""
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "updater-signing-env.ps1")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
if (-not $Semver) {
    $version = Get-Content (Join-Path $repoRoot "version.json") -Raw | ConvertFrom-Json
    $Semver = "$($version.major).$($version.minor).$($version.build)"
}

if (-not (Set-UpdaterSigningEnv)) {
    Write-Error "Updater signing key missing. Run scripts/ensure-updater-keys.ps1"
}

$nsisDir = Join-Path (Join-Path $env:CARGO_TARGET_DIR "release") "bundle\nsis"
$setup = Join-Path $nsisDir "Veyro_${Semver}_x64-setup.exe"
if (-not (Test-Path $setup)) {
    Write-Error "Installer not found: $setup"
}

Push-Location $repoRoot
try {
    Write-Host "Signing $setup ..."
    & (Join-Path $repoRoot "node_modules\.bin\tauri.cmd") signer sign $setup
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    Write-Host "Created $setup.sig"
} finally {
    Pop-Location
}

& (Join-Path $PSScriptRoot "finish-release-artifacts.ps1") -Semver $Semver
