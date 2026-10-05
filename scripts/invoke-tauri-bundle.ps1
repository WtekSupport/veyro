# Release packaging via `tauri build` (embeds frontendDist + NSIS). Do not use `tauri bundle`
# on a raw `cargo build` — that skips the CLI embed path installers expect.

function Invoke-VeyroFrontendBuild {
    npm run build
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

function Invoke-VeyroTauriReleaseBuild {
    param(
        [AllowEmptyString()]
        [string]$Features
    )

    $repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
    $releaseConfig = Join-Path $repoRoot "src-tauri/tauri.release.conf.json"

    $buildArgs = @(
        "exec", "tauri", "--", "build",
        "--bundles", "nsis",
        "--ci",
        "--no-binary-patching",
        "--config", $releaseConfig
    )
    if ($Features) {
        $buildArgs += @("--features", $Features)
    } else {
        $buildArgs += @("--no-default-features")
    }
    npm @buildArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Backward-compatible name used by bundle-only script.
function Invoke-VeyroTauriBundle {
    param(
        [AllowEmptyString()]
        [string]$Features
    )
    Invoke-VeyroTauriReleaseBuild -Features $Features
}
