$ErrorActionPreference = "Continue"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$log = Join-Path $repoRoot "_dev-monitor.log"
function Write-Log([string]$Line) {
    Add-Content -Path $log -Value $Line -Encoding utf8
}
Write-Log "=== dev monitor start $(Get-Date -Format o) ==="
Set-Location $repoRoot
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Write-Log "IsAdmin: $isAdmin"
if (-not $isAdmin) {
    Write-Log "NOTE: npm run tauri:dev will open UAC — approve it in the elevated PowerShell window."
}
npm run tauri:dev 2>&1 | ForEach-Object { Write-Log $_ }
