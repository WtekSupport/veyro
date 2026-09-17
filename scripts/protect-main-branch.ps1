#Requires -Version 5.1
<#
.SYNOPSIS
  Protect main on a personal GitHub repo: CI required, no force-push/delete.

  Personal repos cannot restrict pushes to named users via classic protection;
  only the owner (and collaborators you add) can push. Keep Collaborators empty.

Requires GITHUB_TOKEN, GH_TOKEN, or Git Credential Manager (same as git push).
#>
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Owner = "WtekSupport"
$Repo = "veyro"
$Branch = "main"
$OwnerUserId = 35570810

# GitHub may report checks as job id ("frontend") or "CI / frontend" (workflow name + job).
# Update this list if branch protection blocks merges despite green Actions.
$RequiredCheckContexts = @("frontend", "rust")

function Get-GitHubToken {
    if ($env:GITHUB_TOKEN) { return $env:GITHUB_TOKEN.Trim() }
    if ($env:GH_TOKEN) { return $env:GH_TOKEN.Trim() }

    $fillInput = "protocol=https`nhost=github.com`n`n"
    $fillOutput = $fillInput | & git credential fill 2>$null
    if (-not $fillOutput) {
        throw "No GITHUB_TOKEN/GH_TOKEN and git credential fill failed."
    }
    foreach ($line in $fillOutput -split "`n") {
        if ($line -like "password=*") {
            return $line.Substring("password=".Length)
        }
    }
    throw "Git credential fill did not return a token."
}

$token = Get-GitHubToken
$headers = @{
    Authorization          = "Bearer $token"
    Accept                 = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}

# Classic branch protection (no user restrictions — not supported on personal repos).
$protectionBody = @{
    required_status_checks = @{
        strict   = $true
        contexts = $RequiredCheckContexts
    }
    enforce_admins                = $true
    required_pull_request_reviews = $null
    restrictions                  = $null
    required_linear_history       = $false
    allow_force_pushes            = $false
    allow_deletions               = $false
    block_creations               = $false
} | ConvertTo-Json -Depth 5

$protectionUri = "https://api.github.com/repos/$Owner/$Repo/branches/$Branch/protection"
Invoke-RestMethod -Method Put -Uri $protectionUri -Headers $headers -Body $protectionBody -ContentType "application/json"
Write-Host "Classic protection on ${Branch}: checks [$($RequiredCheckContexts -join ', ')], no force-push/delete, enforce admins."

# Repository ruleset: same rules; only repo owner may bypass (optional extra layer).
$rulesetBody = @{
    name        = "Protect main"
    target      = "branch"
    enforcement = "active"
    conditions  = @{
        ref_name = @{
            include = @("refs/heads/main")
            exclude = @()
        }
    }
    rules = @(
        @{ type = "deletion" }
        @{ type = "non_fast_forward" }
        @{
            type       = "required_status_checks"
            parameters = @{
                strict_required_status_checks_policy = $true
                required_status_checks                 = @(
                    $RequiredCheckContexts | ForEach-Object { @{ context = $_ } }
                )
            }
        }
    )
    bypass_actors = @(
        @{
            actor_id     = $OwnerUserId
            actor_type   = "User"
            bypass_mode  = "always"
        }
    )
} | ConvertTo-Json -Depth 8

$rulesetsUri = "https://api.github.com/repos/$Owner/$Repo/rulesets"
$existingRaw = Invoke-RestMethod -Uri $rulesetsUri -Headers $headers
$existing = @()
if ($null -ne $existingRaw) {
    if ($existingRaw -is [System.Array]) {
        $existing = $existingRaw
    } else {
        $existing = @($existingRaw)
    }
}
$existingRule = $existing | Where-Object { $_.PSObject.Properties.Name -contains "name" -and $_.name -eq "Protect main" } | Select-Object -First 1
if ($existingRule) {
    $updateUri = "https://api.github.com/repos/$Owner/$Repo/rulesets/$($existingRule.id)"
    Invoke-RestMethod -Method Put -Uri $updateUri -Headers $headers -Body $rulesetBody -ContentType "application/json"
    Write-Host "Updated ruleset '$($existingRule.name)' (id $($existingRule.id))."
} else {
    Invoke-RestMethod -Method Post -Uri $rulesetsUri -Headers $headers -Body $rulesetBody -ContentType "application/json"
    Write-Host "Created ruleset 'Protect main'."
}

Write-Host "Done. Confirm: Settings -> Collaborators (only you), Branches / Rulesets."
