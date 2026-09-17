# Auto-updates (Tauri Updater)

Veyro checks [GitHub Releases](https://github.com/WtekSupport/veyro/releases) via `latest.json`, verifies a **minisign** signature, then installs through the Tauri updater (separate installer process on Windows).

## One-time setup (developer)

```powershell
powershell -ExecutionPolicy Bypass -File scripts/ensure-updater-keys.ps1
```

This creates:

- **Private key:** `%USERPROFILE%\.tauri\veyro-updater.key` (backup securely; losing it blocks signed updates for existing installs)
- **Public key:** `src-tauri/updater.pub` (committed to git)

Release builds need the private key in the environment (the build script sets it from the path above when present):

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\veyro-updater.key"
# optional: $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "..."
npm run tauri:build
```

`scripts/sync-updater-config.mjs` enables `createUpdaterArtifacts` and embeds `updater.pub` into `tauri.conf.json` before bundling.

## Release artifacts

After `npm run tauri:build`, under `release/<semver>/`:

| File | Purpose |
|------|---------|
| `Veyro_<ver>_x64-setup.exe` | NSIS installer (manual + updater download) |
| `Veyro_<ver>_x64-setup.exe.sig` | Signature for `latest.json` |
| `latest.json` | Manifest for `releases/latest/download/latest.json` on GitHub |

Upload **setup.exe**, **portable zip**, **setup.exe.sig**, and **latest.json** to the GitHub Release. The `latest` release asset URL must serve the newest `latest.json`.

```powershell
# Uses Git Credential Manager (same as git push), or GITHUB_TOKEN / `gh auth token`
powershell -ExecutionPolicy Bypass -File scripts/publish-github-release-from-gcm.ps1 -Semver 1.7.71
```

## User flow

1. App starts → optional check (Settings → check for updates on startup).
2. If a newer signed version exists → banner “Update available”.
3. User clicks **Update** → download → verify → install → restart.
