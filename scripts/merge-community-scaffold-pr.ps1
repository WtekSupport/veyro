#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. "$PSScriptRoot/github-api.ps1"

$existing = Invoke-GitHubApi -Method Get -Uri "https://api.github.com/repos/WtekSupport/veyro/pulls?head=WtekSupport:oss/community-scaffold&state=open"
$prNumber = $null
if ($existing.Count -gt 0) {
    $prNumber = $existing[0].number
    Write-Host "Using open PR #$prNumber"
} else {
    $pr = Invoke-GitHubApi -Method Post -Uri "https://api.github.com/repos/WtekSupport/veyro/pulls" -Body @{
        title = "Open-source community scaffolding"
        head  = "oss/community-scaffold"
        base  = "main"
        body  = "OSS scaffolding from audit plan. Merge when CI is green."
    }
    $prNumber = $pr.number
    Write-Host "Created PR #$prNumber $($pr.html_url)"
}

$headRef = "oss/community-scaffold"
$deadline = (Get-Date).AddMinutes(25)
while ((Get-Date) -lt $deadline) {
    $runs = Invoke-GitHubApi -Method Get -Uri "https://api.github.com/repos/WtekSupport/veyro/commits/$headRef/check-runs?per_page=100"
    $required = @("frontend", "rust")
    $byName = @{}
    foreach ($cr in $runs.check_runs) {
        if ($required -contains $cr.name) { $byName[$cr.name] = $cr }
    }
    $missing = @($required | Where-Object { -not $byName.ContainsKey($_) })
    if ($missing.Count -gt 0) {
        Write-Host "Waiting for checks: $($missing -join ', ')"
        Start-Sleep -Seconds 30
        continue
    }
    $failed = @($required | Where-Object { $byName[$_].conclusion -eq "failure" })
    if ($failed.Count -gt 0) { throw "CI failed: $($failed -join ', ')" }
    $pending = @($required | Where-Object { $byName[$_].status -ne "completed" })
    if ($pending.Count -eq 0) {
        $bad = @($required | Where-Object { $byName[$_].conclusion -ne "success" })
        if ($bad.Count -gt 0) {
            throw "CI not successful: $($bad -join ', ')"
        }
        Write-Host "Checks frontend + rust: success"
        break
    }
    Write-Host "Checks in progress: $($pending -join ', ')"
    Start-Sleep -Seconds 30
}

if ((Get-Date) -ge $deadline) {
    throw "Timed out waiting for CI success."
}

$merge = Invoke-GitHubApi -Method Put -Uri "https://api.github.com/repos/WtekSupport/veyro/pulls/$prNumber/merge" -Body @{
    merge_method = "merge"
    commit_title = "Merge pull request #$prNumber from oss/community-scaffold"
}
Write-Host "Merged: $($merge.message) sha=$($merge.sha)"
