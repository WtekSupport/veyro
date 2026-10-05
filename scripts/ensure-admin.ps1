# Re-launch a script elevated (Windows UAC). No-op when already admin or not on Windows.
param(
    [Parameter(Mandatory = $true)]
    [string]$CallerScript,
    # By default we wait for the elevated process and exit with its code (callers must not continue unelevated).
    [switch]$NoWait
)

$isWindowsPlatform = ($IsWindows -eq $true) -or ($env:OS -like "*Windows*")
if (-not $isWindowsPlatform) {
    return
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    return
}

if (-not (Test-Path -LiteralPath $CallerScript)) {
    Write-Error "Caller script not found: $CallerScript"
    exit 1
}

$repoRoot = (Resolve-Path (Join-Path (Split-Path -Parent $CallerScript) "..")).Path
$argList = "-NoProfile -ExecutionPolicy Bypass -File `"$CallerScript`""
Write-Host "Requesting administrator privileges for: $CallerScript"

if ($NoWait) {
    try {
        Start-Process `
            -FilePath "powershell.exe" `
            -Verb RunAs `
            -ArgumentList $argList `
            -WorkingDirectory $repoRoot | Out-Null
    } catch {
        Write-Error "Administrator approval was cancelled or denied."
        exit 1
    }
    exit 0
}

try {
    $proc = Start-Process `
        -FilePath "powershell.exe" `
        -Verb RunAs `
        -ArgumentList $argList `
        -WorkingDirectory $repoRoot `
        -Wait `
        -PassThru
    if (-not $proc) {
        Write-Error "Administrator approval was cancelled or denied. Veyro dev must run elevated."
        exit 1
    }
    $code = if ($null -ne $proc.ExitCode) { $proc.ExitCode } else { 1 }
    exit $code
} catch {
    Write-Error "Administrator approval was cancelled or denied. Veyro dev must run elevated (accept UAC or use an elevated PowerShell)."
    exit 1
}
