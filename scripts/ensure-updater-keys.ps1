# Creates minisign key pair for Tauri updater (once) and writes the public key into the repo.
# Private key stays in %USERPROFILE%\.tauri\veyro-updater.key — never commit it.

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$keyPath = Join-Path $env:USERPROFILE ".tauri\veyro-updater.key"
$pubPath = Join-Path $repoRoot "src-tauri\updater.pub"

if (-not (Test-Path $keyPath)) {
    . (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
    New-Item -ItemType Directory -Force -Path (Split-Path $keyPath) | Out-Null
    Write-Host "Generating updater signing keys at $keyPath ..."
    Push-Location $repoRoot
    try {
        $env:CI = "true"
        npx tauri signer generate -w $keyPath --ci
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path "$keyPath.pub")) {
    Write-Error "Public key not found at $keyPath.pub"
}

Copy-Item "$keyPath.pub" $pubPath -Force
Write-Host "Updated $pubPath from signing key."

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    $env:TAURI_SIGNING_PRIVATE_KEY = $keyPath
    Write-Host "Set TAURI_SIGNING_PRIVATE_KEY to key file path for this session."
}
