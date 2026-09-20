# Print last clippy-related lines from a GitHub Actions job log (uses git credential fill).
param(
    [Parameter(Mandatory = $true)]
    [long]$JobId
)

$ErrorActionPreference = "Stop"
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

$token = Get-GitHubToken
$headers = @{
    Authorization          = "Bearer $token"
    Accept                 = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}

$logUrl = (Invoke-RestMethod -Uri "https://api.github.com/repos/WtekSupport/veyro/actions/jobs/$JobId/logs" -Headers $headers -MaximumRedirection 0 -ErrorAction SilentlyContinue)
if (-not $logUrl) {
    $logUrl = "https://api.github.com/repos/WtekSupport/veyro/actions/jobs/$JobId/logs"
}

$zipPath = Join-Path $env:TEMP "veyro-ci-job-$JobId.zip"
Invoke-WebRequest -Uri "https://api.github.com/repos/WtekSupport/veyro/actions/jobs/$JobId/logs" -Headers $headers -OutFile $zipPath
Expand-Archive -Path $zipPath -DestinationPath (Join-Path $env:TEMP "veyro-ci-job-$JobId") -Force
Get-ChildItem (Join-Path $env:TEMP "veyro-ci-job-$JobId") -Recurse -File |
    ForEach-Object { Get-Content $_.FullName } |
    Select-String -Pattern "error:|warning:.*clippy|could not compile" |
    Select-Object -Last 40
