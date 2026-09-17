# Upload a single file to an existing GitHub release.
param(
    [Parameter(Mandatory = $true)][string]$Semver,
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string]$AssetName
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "github-api.ps1")

if (-not (Test-Path $FilePath)) {
    Write-Error "File not found: $FilePath"
}

$repo = "WtekSupport/veyro"
$tag = "v$Semver"
if (-not $AssetName) {
    $AssetName = [IO.Path]::GetFileName($FilePath)
}

$release = Invoke-GitHubApi -Method Get -Uri "https://api.github.com/repos/$repo/releases/tags/$tag"
$existing = @($release.assets | Where-Object { $_.name -eq $AssetName })
foreach ($asset in $existing) {
    Write-Host "Removing existing asset: $($asset.name) (id $($asset.id))"
    Invoke-RestMethod -Method Delete -Uri "https://api.github.com/repos/$repo/releases/assets/$($asset.id)" -Headers (Get-GitHubHeaders)
}

$encodedName = [System.Uri]::EscapeDataString($AssetName)
$uri = "https://uploads.github.com/repos/$repo/releases/$($release.id)/assets?name=$encodedName"
$headers = Get-GitHubHeaders
Write-Host "Uploading $AssetName to $tag ..."
Invoke-RestMethod -Method POST -Uri $uri -Headers $headers -InFile $FilePath -ContentType "application/octet-stream" | Out-Null
Write-Host "Done: https://github.com/$repo/releases/tag/$tag"
