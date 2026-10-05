. (Join-Path $PSScriptRoot "github-api.ps1")
$all = Invoke-GitHubApi -Method Get -Uri "https://api.github.com/repos/WtekSupport/veyro/releases?per_page=5"
$all | ForEach-Object { Write-Host "$($_.tag_name) assets=$($_.assets.Count) draft=$($_.draft)" }
