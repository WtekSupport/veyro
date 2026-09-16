# Uses Git Credential Manager (already used for git push) to obtain a GitHub API token.
param(
    [Parameter(Mandatory = $true)]
    [string]$Semver
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$fillScript = Join-Path $env:TEMP "veyro-gcm-fill.ps1"
@'
$input = @"
protocol=https
host=github.com

"@
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "git"
$psi.Arguments = "credential fill"
$psi.WorkingDirectory = $args[0]
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
$token = $null
foreach ($line in ($out -split "`n")) {
    if ($line -match '^password=(.+)$') {
        $token = $Matches[1].Trim().Trim("`r", "`n")
        break
    }
}
if (-not $token) {
    throw "No GitHub token from credential manager"
}
$env:GITHUB_TOKEN = $token
& (Join-Path $args[0] "scripts\publish-github-release.ps1") -Semver $args[1]
'@ | Set-Content -Path $fillScript -Encoding UTF8

& $fillScript $repoRoot $Semver
