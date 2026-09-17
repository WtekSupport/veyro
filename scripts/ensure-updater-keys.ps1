# Creates minisign key pair for Tauri updater (once) and writes the public key into the repo.
# Private key: %USERPROFILE%\.tauri\veyro-updater.key (never commit)
# Password file (local only): %USERPROFILE%\.tauri\veyro-updater.password

param(
    [switch]$ResetSigningKey
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "updater-signing-env.ps1")

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$paths = Get-UpdaterSigningPaths
$keyPath = $paths.KeyPath
$pubPath = Join-Path $repoRoot "src-tauri\updater.pub"

function New-UpdaterSigningKeyPair {
    . (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
    New-Item -ItemType Directory -Force -Path $paths.Dir | Out-Null
    $passwordBytes = New-Object byte[] 32
    [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($passwordBytes)
    $password = [Convert]::ToBase64String($passwordBytes)

    Write-Host "Generating updater signing keys at $keyPath (password saved locally, no build prompt) ..."
    Push-Location $repoRoot
    try {
        $env:CI = "true"
        $genArgs = @(
            "tauri", "signer", "generate",
            "-w", $keyPath,
            "-f",
            "--password", $password,
            "--ci"
        )
        & npx @genArgs
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    } finally {
        Pop-Location
    }
    Write-UpdaterSigningPasswordFile -Password $password -PasswordPath $paths.PasswordPath
    Write-Host "Wrote signing password to $($paths.PasswordPath) (local only; back up with the .key file)."
}

if ($ResetSigningKey -and (Test-Path $keyPath)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $backup = "$keyPath.lost-password.$stamp"
    Move-Item $keyPath $backup -Force
    if (Test-Path "$keyPath.pub") {
        Move-Item "$keyPath.pub" "$backup.pub" -Force
    }
    Write-Warning "Backed up unusable signing key to $backup"
    Write-Warning "Auto-updates from older installs need one manual install after the next release (new pubkey)."
}

if (-not (Test-Path $keyPath)) {
    New-UpdaterSigningKeyPair
}

if (-not (Test-Path "$keyPath.pub")) {
    Write-Error "Public key not found at $keyPath.pub"
}

Copy-Item "$keyPath.pub" $pubPath -Force
Write-Host "Updated $pubPath from signing key."

if (-not (Set-UpdaterSigningEnv)) {
    Write-Error "Failed to configure TAURI_SIGNING_* environment."
}
Write-Host "Updater signing env ready (password from file or empty; no interactive prompt)."
