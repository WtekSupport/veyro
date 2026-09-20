# Package NSIS installer from an existing release/veyro.exe (no `tauri build` / no llama recompile).
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

. (Join-Path $PSScriptRoot "updater-signing-env.ps1")
& (Join-Path $PSScriptRoot "ensure-updater-keys.ps1")
& node (Join-Path $PSScriptRoot "sync-updater-config.mjs")
& (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
& (Join-Path $PSScriptRoot "ensure-cmake.ps1")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
. (Join-Path $PSScriptRoot "resolve-local-features.ps1")

$features = Resolve-LocalFeatures -RepoRoot $repoRoot
Set-LlamaCppBuildParallelism -RepoRoot $repoRoot | Out-Null

& (Join-Path $PSScriptRoot "stage-llm-dlls.ps1") -RepoRoot $repoRoot -ErrorAction SilentlyContinue
& (Join-Path $PSScriptRoot "stage-sherpa-dlls.ps1") -RepoRoot $repoRoot -Required
& (Join-Path $PSScriptRoot "stage-libtorch-dlls.ps1") -RepoRoot $repoRoot -Profile "release" -Required
& (Join-Path $PSScriptRoot "sync-windows-bundle-resources.ps1") -RepoRoot $repoRoot

$exe = Join-Path $env:CARGO_TARGET_DIR "release\veyro.exe"
if (-not (Test-Path $exe)) {
    Write-Error "Missing $exe; run a release cargo build first."
}
Write-Host "Using existing binary: $exe"

if (-not (Set-UpdaterSigningEnv)) {
    Write-Error "Updater signing env missing."
}

npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

npm exec tauri -- bundle --features $features --bundles nsis --ci
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$version = Get-Content (Join-Path $repoRoot "version.json") -Raw | ConvertFrom-Json
$semver = "$($version.major).$($version.minor).$($version.build)"
& (Join-Path $PSScriptRoot "finish-release-artifacts.ps1") -Semver $semver
