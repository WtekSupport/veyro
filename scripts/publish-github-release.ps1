# Upload release/<semver>/ artifacts to GitHub Releases (Windows x64).
# Requires: GITHUB_TOKEN (repo scope) or `gh auth token` if GitHub CLI is installed.

param(
    [Parameter(Mandatory = $true)]
    [string]$Semver
)

$ErrorActionPreference = "Stop"
$repo = "WtekSupport/veyro"
$tag = "v$Semver"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$dir = Join-Path (Join-Path $root "release") $Semver

if (-not (Test-Path $dir)) {
    Write-Error "Release folder not found: $dir"
}

$token = $env:GITHUB_TOKEN
if (-not $token) {
    $gh = Get-Command gh -ErrorAction SilentlyContinue
    if ($gh) {
        $token = (& gh auth token 2>$null).Trim()
    }
}
if (-not $token) {
    Write-Error "Set GITHUB_TOKEN or install and authenticate GitHub CLI (gh auth login)."
}

$headers = @{
    Authorization = "Bearer $token"
    Accept        = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}

function Invoke-GhApi {
    param([string]$Method, [string]$Uri, $Body = $null, [hashtable]$ExtraHeaders = @{})
    $h = $headers.Clone()
    foreach ($k in $ExtraHeaders.Keys) { $h[$k] = $ExtraHeaders[$k] }
    $params = @{ Method = $Method; Uri = $Uri; Headers = $h }
    if ($null -ne $Body) { $params.Body = ($Body | ConvertTo-Json -Depth 8 -Compress) }
    Invoke-RestMethod @params
}

Write-Host "Creating release $tag ..."
try {
    Invoke-GhApi -Method POST -Uri "https://api.github.com/repos/$repo/releases" -Body @{
        tag_name   = $tag
        name       = "Veyro $Semver"
        body       = "Windows installer, portable build, and updater manifest (`latest.json`)."
        draft      = $false
        prerelease = $false
    } | Out-Null
} catch {
    if ($_.Exception.Response.StatusCode.value__ -eq 422) {
        Write-Host "Release $tag already exists; uploading assets."
    } else {
        throw
    }
}

$release = Invoke-GhApi -Method GET -Uri "https://api.github.com/repos/$repo/releases/tags/$tag"
$uploadBase = "https://uploads.github.com/repos/$repo/releases/$($release.id)/assets"

$files = @(
    "Veyro_${Semver}_x64-setup.exe",
    "Veyro_${Semver}_x64-setup.exe.sig",
    "Veyro_${Semver}_x64-portable.zip",
    "latest.json"
)

foreach ($name in $files) {
    $path = Join-Path $dir $name
    if (-not (Test-Path $path)) {
        Write-Warning "Skip missing: $name"
        continue
    }
    Write-Host "Uploading $name ..."
    $encodedName = [System.Uri]::EscapeDataString($name)
    $uri = "${uploadBase}?name=$encodedName"
    Invoke-RestMethod -Method POST -Uri $uri -Headers @{
        Authorization = "Bearer $token"
        Accept        = "application/vnd.github+json"
    } -InFile $path -ContentType "application/octet-stream" | Out-Null
}

Write-Host "Done: https://github.com/$repo/releases/tag/$tag"
