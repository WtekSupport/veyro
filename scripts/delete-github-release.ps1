# Delete a GitHub Release and its tag (remote + local).
param(
    [Parameter(Mandatory = $true)]
    [string]$Semver
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$repo = "WtekSupport/veyro"
$tag = "v$Semver"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Get-GitHubToken {
    $input = @"
protocol=https
host=github.com

"@
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "git"
    $psi.Arguments = "credential fill"
    $psi.WorkingDirectory = $repoRoot
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.UseShellExecute = $false
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Write($input)
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEnd()
    $p.WaitForExit()
    foreach ($line in ($out -split "`n")) {
        if ($line -match '^password=(.+)$') {
            return $Matches[1].Trim().Trim("`r", "`n")
        }
    }
    throw "No GitHub token"
}

$h = @{
    Authorization          = "Bearer $(Get-GitHubToken)"
    Accept                 = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}

try {
    $release = Invoke-RestMethod -Method GET -Uri "https://api.github.com/repos/$repo/releases/tags/$tag" -Headers $h
    Write-Host "Deleting release $tag (id $($release.id)) ..."
    Invoke-RestMethod -Method DELETE -Uri "https://api.github.com/repos/$repo/releases/$($release.id)" -Headers $h | Out-Null
} catch {
    Write-Warning "Release $tag not found or already deleted: $($_.Exception.Message)"
}

Write-Host "Deleting remote tag $tag ..."
& git -C $repoRoot push origin ":refs/tags/$tag" 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Warning "Remote tag $tag may already be gone."
}

if (Test-Path (Join-Path $repoRoot ".git\refs\tags\$tag")) {
    Remove-Item (Join-Path $repoRoot ".git\refs\tags\$tag") -Force
}
& git -C $repoRoot tag -d $tag 2>$null

Write-Host "Done: removed GitHub release and tag $tag"
