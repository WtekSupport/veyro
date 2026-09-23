#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-GitHubToken {
    if ($env:GITHUB_TOKEN) { return $env:GITHUB_TOKEN.Trim() }
    if ($env:GH_TOKEN) { return $env:GH_TOKEN.Trim() }
    $fillInput = "protocol=https`nhost=github.com`n`n"
    $fillOutput = $fillInput | & git credential fill 2>$null
    foreach ($line in $fillOutput -split "`n") {
        if ($line -like "password=*") {
            return $line.Substring("password=".Length)
        }
    }
    throw "No GitHub token available."
}

function Get-GitHubHeaders {
    $token = Get-GitHubToken
    return @{
        Authorization          = "Bearer $token"
        Accept                 = "application/vnd.github+json"
        "X-GitHub-Api-Version" = "2022-11-28"
    }
}

function Invoke-GitHubApi {
    param(
        [Parameter(Mandatory)] [ValidateSet("Get", "Post", "Put", "Patch")] [string] $Method,
        [Parameter(Mandatory)] [string] $Uri,
        [object] $Body
    )
    $headers = Get-GitHubHeaders
    $params = @{
        Method  = $Method
        Uri     = $Uri
        Headers = $headers
    }
    if ($null -ne $Body) {
        $jsonBody = ($Body | ConvertTo-Json -Depth 10 -Compress)
        $params.Body = [System.Text.Encoding]::UTF8.GetBytes($jsonBody)
        $params.ContentType = "application/json; charset=utf-8"
    }
    return Invoke-RestMethod @params
}
