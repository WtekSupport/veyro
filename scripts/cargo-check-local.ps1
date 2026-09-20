$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot
. (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "features=$features"
Push-Location (Join-Path $repoRoot "src-tauri")
try {
    cargo check @((Get-CargoFeatureArgs -Features $features))
} finally {
    Pop-Location
}
