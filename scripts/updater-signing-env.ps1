# Shared env for Tauri updater (minisign) — dot-source from build/release scripts.

function Get-UpdaterSigningPaths {
    $dir = Join-Path $env:USERPROFILE ".tauri"
    return [ordered]@{
        Dir          = $dir
        KeyPath      = Join-Path $dir "veyro-updater.key"
        KeyPubPath   = Join-Path $dir "veyro-updater.key.pub"
        PasswordPath = Join-Path $dir "veyro-updater.password"
    }
}

function Get-UpdaterSigningPassword {
    param([string]$PasswordPath)

    if ($null -ne $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        return $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
    }
    if (Test-Path $PasswordPath) {
        $raw = Get-Content $PasswordPath -Raw -ErrorAction SilentlyContinue
        if ($null -eq $raw) {
            return ""
        }
        return $raw.TrimEnd("`r", "`n")
    }
    return ""
}

function Set-UpdaterSigningEnv {
    $paths = Get-UpdaterSigningPaths
    if (-not (Test-Path $paths.KeyPath)) {
        return $false
    }

    # Wrong pattern from old docs: path stored in TAURI_SIGNING_PRIVATE_KEY breaks non-interactive sign.
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
    $userKey = [Environment]::GetEnvironmentVariable("TAURI_SIGNING_PRIVATE_KEY", "User")
    if ($userKey -and $userKey -match '\.key$') {
        Write-Warning "Remove User env var TAURI_SIGNING_PRIVATE_KEY (file path). Scripts use TAURI_SIGNING_PRIVATE_KEY_PATH + .tauri\veyro-updater.password."
    }

    $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $paths.KeyPath
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-UpdaterSigningPassword -PasswordPath $paths.PasswordPath
    return $true
}

function Write-UpdaterSigningPasswordFile {
    param(
        [string]$Password,
        [string]$PasswordPath = (Get-UpdaterSigningPaths).PasswordPath
    )

    New-Item -ItemType Directory -Force -Path (Split-Path $PasswordPath) | Out-Null
    Set-Content -Path $PasswordPath -Value $Password -NoNewline -Encoding UTF8
}
