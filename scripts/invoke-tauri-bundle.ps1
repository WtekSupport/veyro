# Bundle NSIS from a release `cargo build`. Frontend must be built before `cargo build`
# (tauri-build embeds `frontendDist` at compile time).

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

    $bundleArgs = @("exec", "tauri", "--", "bundle", "--bundles", "nsis", "--ci", "--no-binary-patching")
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
