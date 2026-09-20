# Re-launch a script elevated (Windows UAC). No-op when already admin.
param(
    [Parameter(Mandatory = $true)]
    [string]$CallerScript,
    [switch]$Wait
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
}

$repoRoot = (Resolve-Path (Join-Path (Split-Path -Parent $CallerScript) "..")).Path
$argList = "-ExecutionPolicy Bypass -File `"$CallerScript`""
Write-Host "Requesting administrator privileges for: $CallerScript"
if ($Wait) {
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
        exit $(if ($null -ne $proc.ExitCode) { $proc.ExitCode } else { 1 })
    } catch {
        Write-Error "Administrator approval was cancelled or denied. Veyro dev must run elevated (accept UAC or use an elevated PowerShell)."
        exit 1
    }
}

try {
    Start-Process `
        -FilePath "powershell.exe" `
        -Verb RunAs `
        -ArgumentList $argList `
        -WorkingDirectory $repoRoot
} catch {
    Write-Error "Administrator approval was cancelled or denied."
    exit 1
}
exit 0
