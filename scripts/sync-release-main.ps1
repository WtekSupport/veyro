# Merge release PR when checks pass; sync local main; retag v* on release head if needed.
param(
    [int]$PullNumber = 0,
    [string]$HeadBranch = "release/1.8.8"
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$repo = "WtekSupport/veyro"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

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
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Write($input)
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEnd()
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) {
        throw "git credential fill failed"
    }
    foreach ($line in ($out -split "`n")) {
        if ($line -match '^password=(.+)$') {
            return $Matches[1].Trim().Trim("`r", "`n")
        }
    }
    throw "No GitHub token from credential manager"
}

function Invoke-GhApi {
    param([string]$Method, [string]$Uri, $Body = $null)
    $params = @{ Method = $Method; Uri = $Uri; Headers = $script:GhHeaders }
    if ($null -ne $Body) {
        $params.Body = ($Body | ConvertTo-Json -Compress)
        $params.ContentType = "application/json"
    }
    Invoke-RestMethod @params
}

$script:GhHeaders = @{
    Authorization          = "Bearer $(Get-GitHubToken)"
    Accept                 = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}

git fetch origin

$pull = $null
if ($PullNumber -gt 0) {
    try {
        $pull = Invoke-GhApi -Method GET -Uri "https://api.github.com/repos/$repo/pulls/$PullNumber"
    } catch {
        $pull = $null
    }
}

if (-not $pull -or $pull.state -ne "open") {
    $open = Invoke-GhApi -Method GET -Uri "https://api.github.com/repos/$repo/pulls?head=WtekSupport:$HeadBranch&state=open"
    if ($open -and $open.Count -gt 0) {
        $pull = $open[0]
        $PullNumber = $pull.number
    }
}

if (-not $pull -or $pull.state -ne "open") {
    $version = Get-Content (Join-Path $repoRoot "version.json") -Raw | ConvertFrom-Json
    $semver = "$($version.major).$($version.minor).$($version.build)"
    Write-Host "Creating PR $HeadBranch -> main ..."
    $pull = Invoke-GhApi -Method POST -Uri "https://api.github.com/repos/$repo/pulls" -Body @{
        title = "Release $semver"
        head  = $HeadBranch
        base  = "main"
        body  = "Release $semver. See CHANGELOG.md and GitHub release v$semver."
    }
    $PullNumber = $pull.number
    Write-Host "Created PR #$PullNumber $($pull.html_url)"
}

if ($pull -and $pull.state -eq "open") {
    Write-Host "PR #$PullNumber $($pull.html_url)"
    $sha = $pull.head.sha
    $deadline = (Get-Date).AddMinutes(25)
    while ((Get-Date) -lt $deadline) {
        $runs = Invoke-GhApi -Method GET -Uri "https://api.github.com/repos/$repo/commits/$sha/check-runs?per_page=100"
        $required = @(
            $runs.check_runs |
                Where-Object { $_.name -in @("rust", "frontend") } |
                Group-Object -Property name |
                ForEach-Object {
                    $_.Group | Sort-Object { [datetime]$_.started_at } -Descending | Select-Object -First 1
                }
        )
        if ($required.Count -eq 0) {
            $combined = Invoke-GhApi -Method GET -Uri "https://api.github.com/repos/$repo/commits/$sha/status"
            Write-Host "Checks: combined=$($combined.state)"
            if ($combined.state -eq "success") { break }
            if ($combined.state -eq "failure") { throw "CI failed on $sha" }
        } else {
            $pending = @($required | Where-Object { $_.status -ne "completed" })
            $bad = @($required | Where-Object { $_.conclusion -in @("failure", "cancelled", "timed_out") })
            foreach ($r in $required) {
                Write-Host "  $($r.name): $($r.status) $($r.conclusion)"
            }
            if ($bad.Count -gt 0) { throw "Required checks failed" }
            if ($pending.Count -eq 0) { break }
        }
        Start-Sleep -Seconds 25
    }

    $prFresh = Invoke-GhApi -Method GET -Uri "https://api.github.com/repos/$repo/pulls/$PullNumber"
    if ($prFresh.mergeable -eq $false) {
        throw "PR #$PullNumber is not mergeable (conflicts?)."
    }

    Write-Host "Merging PR #$PullNumber ..."
    $merge = Invoke-GhApi -Method PUT -Uri "https://api.github.com/repos/$repo/pulls/$PullNumber/merge" -Body @{
        merge_method = "merge"
        commit_title = "Merge $HeadBranch into main"
    }
    Write-Host $merge.message
} else {
    Write-Host "No open PR for $HeadBranch (main may already include release)."
}

git fetch origin
git checkout main
git pull origin main

$version = Get-Content (Join-Path $repoRoot "version.json") -Raw | ConvertFrom-Json
$semver = "$($version.major).$($version.minor).$($version.build)"
$tag = "v$semver"
$headSha = (git rev-parse HEAD).Trim()
$remoteTagSha = (git ls-remote origin "refs/tags/$tag" 2>$null) -replace ".*\t", ""

if ($remoteTagSha -ne $headSha) {
    Write-Host "Moving tag $tag to main HEAD $headSha (was $remoteTagSha)"
    git tag -f $tag
    git push origin $tag --force
} else {
    Write-Host "Tag $tag already points at main HEAD."
}

Write-Host "Synced: origin/main @ $(git rev-parse --short HEAD)"
