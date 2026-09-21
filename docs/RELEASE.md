# Publishing a release

Installers are **not** stored in git. Each `npm run tauri:build` bumps the patch build, bundles NSIS, and copies artifacts into a versioned folder under `release/`.

## Windows (full stack)

```powershell
npm run tauri:build
```

Output layout (gitignored):

```text
release/
  LATEST.txt              # current semver, one line
  1.7.66/
    Veyro_1.7.66_x64-setup.exe
    Veyro_1.7.66_x64-portable.zip
  1.7.67/
    Veyro_1.7.67_x64-setup.exe
    Veyro_1.7.67_x64-setup.exe.sig
    latest.json
    Veyro_1.7.67_x64-portable.zip
    ...
```

NSIS is also built under `%CARGO_TARGET_DIR%\release\bundle\nsis\` (default `C:\veyro-target\release\bundle\nsis\`).

Signed builds and the in-app updater: [docs/UPDATER.md](UPDATER.md).

## Silero TE on-demand assets

Installers no longer bundle `silero-te` weights. The app downloads them into the user models folder.

**One-time publish** (required before users can download in release builds):

```powershell
python scripts/extract-silero-te.py
powershell -ExecutionPolicy Bypass -File scripts/package-silero-te-release.ps1 -RepoRoot .
```

This creates or updates GitHub release **`silero-te-assets-v1`** with flat assets: `model.pt`, `tokenizer.pt`, `meta.json` (~88 MB total). Needs `gh auth login`.

Override URLs for testing with `VEYRO_SILERO_TE_BASE_URL`. Debug builds can seed from `src-tauri/resources/silero-te` after extract when the release is not published yet.

## Tag and upload

The only git remote is **GitHub** (`origin` → `https://github.com/WtekSupport/veyro.git`). Do not add GitLab or other remotes; CI, releases, and the updater manifest all use GitHub.

```powershell
# semver matches version.json after build
git tag v1.7.66
git push origin main
git push origin v1.7.66
```

Upload the files from `release/<semver>/` to the matching [GitHub Release](https://github.com/WtekSupport/veyro/releases). Remove superseded assets on the Release page when replacing builds.

For automatic updates, each release must include:

| Asset | Purpose |
|-------|---------|
| `Veyro_<ver>_x64-setup.exe` | Installer (manual + updater download) |
| `Veyro_<ver>_x64-setup.exe.sig` | Signature (reference; embedded in manifest) |
| **`latest.json`** | Updater manifest — name the file exactly `latest.json` so `…/releases/latest/download/latest.json` works |

Tag the release as `v<semver>` (e.g. `v1.7.67`) so download URLs in `latest.json` match the uploaded setup exe.
