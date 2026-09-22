param(
    [Parameter(Mandatory = $true)]
    [string] $Semver
)

# Publish a tagged GitHub Release from release/<semver>/ artifacts.
# Requires: gh auth login (repo + write:packages if needed)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$tag = "v$Semver"
$dir = Join-Path $repoRoot "release\$Semver"

$ghCmd = Get-Command gh -ErrorAction SilentlyContinue
if ($ghCmd) {
    $gh = $ghCmd.Source
} elseif (Test-Path (Join-Path $repoRoot ".tools\gh\gh.exe")) {
    $gh = Join-Path $repoRoot ".tools\gh\gh.exe"
} else {
    Write-Error "Install GitHub CLI: https://cli.github.com/"
}

& $gh auth status | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Authenticate first: & `"$gh`" auth login"
    exit 1
}

$files = @(
    (Join-Path $dir "Veyro_${Semver}_x64-setup.exe"),
    (Join-Path $dir "Veyro_${Semver}_x64-setup.exe.sig"),
    (Join-Path $dir "latest.json"),
    (Join-Path $dir "Veyro_${Semver}_x64-portable.zip")
)
foreach ($f in $files) {
    if (-not (Test-Path $f)) {
        Write-Error "Missing: $f (run npm run tauri:build)"
    }
}

$notes = Join-Path $dir "RELEASE_NOTES.md"
if (-not (Test-Path $notes)) {
    Write-Error "Missing: $notes"
}

$releaseView = & $gh release view $tag --repo WtekSupport/veyro 2>&1
if ($LASTEXITCODE -eq 0 -and $releaseView -notmatch "release not found") {
    Write-Host "Uploading assets to existing $tag..."
    & $gh release upload $tag @files --repo WtekSupport/veyro --clobber
    exit $LASTEXITCODE
}

Write-Host "Creating $tag..."
& $gh release create $tag @files `
    --repo WtekSupport/veyro `
    --title "Veyro $Semver" `
    --notes-file $notes
exit $LASTEXITCODE
