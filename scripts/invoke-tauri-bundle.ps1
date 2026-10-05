# Bundle installers from an existing release binary (no extra `cargo build`).
# Run Invoke-VeyroFrontendBuild before the last `cargo build` — tauri embeds `dist` at compile time.

function Invoke-VeyroFrontendBuild {
    npm run build
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

function Invoke-VeyroTauriBundle {
    param(
        [AllowEmptyString()]
        [string]$Features
    )

    $bundleArgs = @("exec", "tauri", "--", "bundle", "--ci")
    if ($Features) {
        $bundleArgs += @("--features", $Features)
    } else {
        $bundleArgs += @("--no-default-features")
    }
    npm @bundleArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}
