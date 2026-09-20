# Re-bundle NSIS after staging native DLLs (no full llama prebuild). Use when release/veyro.exe is already built.
param(
    [switch]$SkipCargo
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

. (Join-Path $PSScriptRoot "updater-signing-env.ps1")
& (Join-Path $PSScriptRoot "ensure-updater-keys.ps1")
& node (Join-Path $PSScriptRoot "sync-updater-config.mjs")
& (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
$parallel = Set-LlamaCppBuildParallelism -RepoRoot $repoRoot

if ($features -match "local-llm") {
    & (Join-Path $PSScriptRoot "stage-llm-dlls.ps1") -RepoRoot $repoRoot
}
if ($features -match "local-sherpa-stt") {
    & (Join-Path $PSScriptRoot "stage-sherpa-dlls.ps1") -RepoRoot $repoRoot -Required
}
& (Join-Path $PSScriptRoot "sync-windows-bundle-resources.ps1") -RepoRoot $repoRoot

if (-not $SkipCargo) {
    Push-Location (Join-Path $repoRoot "src-tauri")
    try {
        $featureArgs = Get-CargoFeatureArgs -Features $features
        cargo build --release @featureArgs -j $parallel
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
}

if (-not (Set-UpdaterSigningEnv)) {
    Write-Error "Updater signing env missing."
}
$bundleArgs = @("run", "tauri", "build", "--")
if ($features) { $bundleArgs += @("--features", $features) } else { $bundleArgs += @("--no-default-features") }
$bundleArgs += @("--", "-j", $parallel)
npm @bundleArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$version = Get-Content (Join-Path $repoRoot "version.json") -Raw | ConvertFrom-Json
$semver = "$($version.major).$($version.minor).$($version.build)"
& (Join-Path $PSScriptRoot "finish-release-artifacts.ps1") -Semver $semver
