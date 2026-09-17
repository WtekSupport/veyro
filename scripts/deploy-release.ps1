# Full Windows release: commit (optional), build, tag, push, publish to GitHub.
param(
    [switch]$SkipCommit,
    [switch]$SkipBuild,
    [switch]$SkipPush
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

& (Join-Path $PSScriptRoot "ensure-updater-keys.ps1")

if (-not $SkipCommit) {
    git add CHANGELOG.md package.json package-lock.json version.json src src-tauri scripts docs
    git add -u
    $status = git status --porcelain
    if ($status) {
        git commit -m @"
Prepare release notes and app changes for deployment.

CHANGELOG documents pause punctuation, Standard/Expert text mode mapping, Russian numerals, and UI fixes.
"@
    }
}

if (-not $SkipBuild) {
    npm run tauri:build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    $version = Get-Content (Join-Path $root "version.json") -Raw | ConvertFrom-Json
    $semver = "$($version.major).$($version.minor).$($version.build)"
    & (Join-Path $PSScriptRoot "finish-release-artifacts.ps1") -Semver $semver
}

$version = Get-Content (Join-Path $root "version.json") -Raw | ConvertFrom-Json
$semver = "$($version.major).$($version.minor).$($version.build)"

git add package.json package-lock.json version.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src/generated/version.ts
$status = git status --porcelain
if ($status) {
    git commit -m "Bump version to $semver for release."
}

$tag = "v$semver"
git tag -f $tag

if (-not $SkipPush) {
    git push origin main
    git push origin $tag --force
    & (Join-Path $PSScriptRoot "publish-github-release-from-gcm.ps1") -Semver $semver
}

Write-Host "Release $semver ready: release/$semver/"
