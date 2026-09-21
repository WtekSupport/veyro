# Publish Silero TE assets to GitHub release tag silero-te-assets-v1 (see model_store.rs).
param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [string]$Repo = "WtekSupport/veyro",
    [string]$Tag = "silero-te-assets-v1"
)

$ErrorActionPreference = "Stop"

$assetsDir = Join-Path (Join-Path $RepoRoot "src-tauri") "resources\silero-te"
$required = @("model.pt", "tokenizer.pt", "meta.json")

foreach ($name in $required) {
    $path = Join-Path $assetsDir $name
    if (-not (Test-Path $path)) {
        Write-Error "Missing $path — run: python scripts/extract-silero-te.py"
    }
}

$paths = $required | ForEach-Object { Join-Path $assetsDir $_ }

Write-Host "Assets ready in $assetsDir"

$gh = Get-Command gh -ErrorAction SilentlyContinue
if (-not $gh) {
    Write-Host ""
    Write-Host "Install GitHub CLI (gh) and run: gh auth login"
    Write-Host "Then upload flat assets to release tag: $Tag"
    foreach ($path in $paths) {
        Write-Host "  - $path"
    }
    Write-Host ""
    Write-Host "Example:"
    Write-Host "  gh release create $Tag $($paths -join ' ') --repo $Repo --title `"Silero TE assets`" --notes `"On-demand Silero TE model files for Veyro`""
    exit 0
}

$viewArgs = @("release", "view", $Tag, "--repo", $Repo)
& gh @viewArgs 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Creating release $Tag on $Repo ..."
    & gh release create $Tag @paths --repo $Repo --title "Silero TE assets" --notes "On-demand Silero TE model files for Veyro (model.pt, tokenizer.pt, meta.json)."
} else {
    Write-Host "Updating assets on release $Tag ..."
    & gh release upload $Tag @paths --repo $Repo --clobber
}

if ($LASTEXITCODE -ne 0) {
    Write-Error "gh release upload failed ($LASTEXITCODE)"
}

Write-Host "Done. Verify:"
Write-Host "  https://github.com/$Repo/releases/tag/$Tag"
