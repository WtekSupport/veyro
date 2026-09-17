# Re-launch a script elevated (Windows UAC). No-op when already admin.
param(
    [Parameter(Mandatory = $true)]
    [string]$CallerScript,
    [switch]$Wait
)

if (-not $IsWindows -and $env:OS -notlike "*Windows*") {
    return
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    return
}

if (-not (Test-Path -LiteralPath $CallerScript)) {
    Write-Error "Caller script not found: $CallerScript"
}

$argList = "-ExecutionPolicy Bypass -File `"$CallerScript`""
Write-Host "Requesting administrator privileges for: $CallerScript"
if ($Wait) {
    Start-Process -FilePath "powershell.exe" -Verb RunAs -ArgumentList $argList -Wait
    exit $LASTEXITCODE
}

Start-Process -FilePath "powershell.exe" -Verb RunAs -ArgumentList $argList
exit 0
