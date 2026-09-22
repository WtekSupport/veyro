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
    $changelogPath = Join-Path $repoRoot "CHANGELOG.md"
    $changelog = Get-Content $changelogPath -Raw
    $pattern = "(?s)## \[$([regex]::Escape($Semver))\][^\r\n]*\r?\n(.*?)(?=\r?\n## \[|\z)"
    if ($changelog -match $pattern) {
        $section = "## [$Semver]" + [Environment]::NewLine + $Matches[1].TrimEnd()
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        Set-Content -Path $notes -Value $section -Encoding UTF8
        Write-Host "Wrote $notes from CHANGELOG.md"
    } else {
        Write-Error "Missing: $notes (no ## [$Semver] section in CHANGELOG.md)"
    }
}

function Publish-ViaGitHubApi {
    . (Join-Path $PSScriptRoot "github-api.ps1")
    $repo = "WtekSupport/veyro"
    $notesBody = (Get-Content $notes -Raw).Trim()
    $release = $null
    try {
        $release = Invoke-GitHubApi -Method Get -Uri "https://api.github.com/repos/$repo/releases/tags/$tag"
        Write-Host "Release $tag exists (id $($release.id)); uploading assets..."
    } catch {
        Write-Host "Creating GitHub release $tag via API..."
        $release = Invoke-GitHubApi -Method Post -Uri "https://api.github.com/repos/$repo/releases" -Body @{
            tag_name = $tag
            name     = "Veyro $Semver"
            body     = $notesBody
        }
    }

    foreach ($f in $files) {
        $assetName = [IO.Path]::GetFileName($f)
        $existing = @($release.assets | Where-Object { $_.name -eq $assetName })
        foreach ($asset in $existing) {
            Write-Host "Removing existing asset: $($asset.name)"
            Invoke-RestMethod -Method Delete -Uri "https://api.github.com/repos/$repo/releases/assets/$($asset.id)" -Headers (Get-GitHubHeaders)
        }
        $encodedName = [System.Uri]::EscapeDataString($assetName)
        $uri = "https://uploads.github.com/repos/$repo/releases/$($release.id)/assets?name=$encodedName"
        Write-Host "Uploading $assetName ..."
        Invoke-RestMethod -Method Post -Uri $uri -Headers (Get-GitHubHeaders) -InFile $f -ContentType "application/octet-stream" | Out-Null
    }
    Write-Host "Published: https://github.com/$repo/releases/tag/$tag"
}

$gh = $null
$ghCmd = Get-Command gh -ErrorAction SilentlyContinue
if ($ghCmd) {
    $gh = $ghCmd.Source
} elseif (Test-Path (Join-Path $repoRoot ".tools\gh\gh.exe")) {
    $gh = Join-Path $repoRoot ".tools\gh\gh.exe"
}

if ($gh) {
    & $gh auth status 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        $releaseView = & $gh release view $tag --repo WtekSupport/veyro 2>&1
        if ($LASTEXITCODE -eq 0 -and $releaseView -notmatch "release not found") {
            Write-Host "Uploading assets to existing $tag..."
            & $gh release upload $tag @files --repo WtekSupport/veyro --clobber
            exit $LASTEXITCODE
        }

        Write-Host "Creating $tag with gh..."
        & $gh release create $tag @files `
            --repo WtekSupport/veyro `
            --title "Veyro $Semver" `
            --notes-file $notes
        exit $LASTEXITCODE
    }
    Write-Warning "gh auth not configured; falling back to GitHub API + git credentials."
}

Publish-ViaGitHubApi
