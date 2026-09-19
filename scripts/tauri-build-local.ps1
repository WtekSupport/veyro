# Release build with local Whisper (whisper-rs). Ensures CMake is available first.

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

& node (Join-Path $PSScriptRoot "bump-version.mjs") prod
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

try {
    & (Join-Path $PSScriptRoot "ensure-updater-keys.ps1")
} catch {
    Write-Warning "Updater signing keys not configured: $_"
}
& node (Join-Path $PSScriptRoot "sync-updater-config.mjs")
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& (Join-Path $PSScriptRoot "ensure-cmake.ps1")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")

& (Join-Path $PSScriptRoot "ensure-rust-path.ps1")

$cmakeBin = Join-Path (Join-Path (Join-Path $repoRoot ".tools") "cmake") "bin"
if (Test-Path (Join-Path $cmakeBin "cmake.exe")) {
    $env:PATH = "$cmakeBin;$env:PATH"
}

. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
$parallel = Set-LlamaCppBuildParallelism -RepoRoot $repoRoot

$featureArgs = @()
if ($features) {
    $featureArgs = @("--features", $features)
} else {
    $featureArgs = @("--no-default-features")
}

function Invoke-ReleaseBuild {
    Push-Location (Join-Path $repoRoot "src-tauri")
    try {
        cargo build --release @featureArgs -j $parallel
        return $LASTEXITCODE
    } finally {
        Pop-Location
    }
}

Write-Host "Building Rust release with '$features' (output: $env:CARGO_TARGET_DIR)..."
$buildExit = Invoke-ReleaseBuild

if ($buildExit -ne 0 -and $features -match "local-llm") {
    Write-Warning "Initial cargo build failed ($buildExit) - finishing llama.cpp cmake install and retrying..."
    & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Profile "release" -Parallel $parallel
    $buildExit = Invoke-ReleaseBuild
}

if ($buildExit -ne 0) {
    exit $buildExit
}

if ($features -match "local-llm") {
    & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Profile "release" -Parallel $parallel
    & (Join-Path $PSScriptRoot "stage-llm-dlls.ps1") -RepoRoot $repoRoot -Required
    & (Join-Path $PSScriptRoot "sync-windows-bundle-resources.ps1") -RepoRoot $repoRoot
}

Write-Host "Building Veyro release binary..."
Push-Location (Join-Path $repoRoot "src-tauri")
try {
    cargo build --release @featureArgs -j $parallel
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Write-Host "Bundling installer..."
$bundleArgs = @("run", "tauri", "build", "--")
if ($features) {
    $bundleArgs += @("--features", $features)
} else {
    $bundleArgs += @("--no-default-features")
}
$bundleArgs += @("--", "-j", $parallel)
npm @bundleArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

function Get-ProjectSemver {
    param([string]$Root)
    $versionPath = Join-Path $Root "version.json"
    $version = Get-Content $versionPath -Raw | ConvertFrom-Json
    return "$($version.major).$($version.minor).$($version.build)"
}

function Publish-ReleaseArtifacts {
    param(
        [string]$Root,
        [string]$ReleaseDir,
        [string]$Semver,
        [string]$Features
    )

    $versionOut = Join-Path (Join-Path $Root "release") $Semver
    New-Item -ItemType Directory -Force -Path $versionOut | Out-Null

    $nsisDir = Join-Path (Join-Path $ReleaseDir "bundle") "nsis"
    if (Test-Path $nsisDir) {
        $setup = Get-ChildItem $nsisDir -Filter "Veyro_${Semver}_x64-setup.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $setup) {
            $setup = Get-ChildItem $nsisDir -Filter "*setup.exe" |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
        }
        if ($setup) {
            $destSetup = Join-Path $versionOut $setup.Name
            Copy-Item $setup.FullName $destSetup -Force
            Write-Host "Release installer: $destSetup"

            $sigSource = "$($setup.FullName).sig"
            if (Test-Path $sigSource) {
                $destSig = Join-Path $versionOut "$($setup.Name).sig"
                Copy-Item $sigSource $destSig -Force
                Write-Host "Release signature: $destSig"

                $signature = (Get-Content $sigSource -Raw).Trim()
                $downloadUrl =
                    "https://github.com/WtekSupport/veyro/releases/download/v$Semver/$($setup.Name)"
                $manifest = [ordered]@{
                    version   = $Semver
                    notes     = "Veyro $Semver"
                    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
                    platforms = [ordered]@{
                        "windows-x86_64" = [ordered]@{
                            url       = $downloadUrl
                            signature = $signature
                        }
                    }
                }
                $latestJson = Join-Path $versionOut "latest.json"
                $jsonText = ($manifest | ConvertTo-Json -Depth 6)
                $utf8NoBom = New-Object System.Text.UTF8Encoding $false
                [System.IO.File]::WriteAllText($latestJson, $jsonText, $utf8NoBom)
                Write-Host "Release manifest: $latestJson"
            } else {
                Write-Warning "No .sig file for updater (set TAURI_SIGNING_PRIVATE_KEY before build)."
            }
        } else {
            Write-Warning "No NSIS setup.exe found under $nsisDir"
        }
    }

    $veyroExe = Join-Path $ReleaseDir "veyro.exe"
    if ((Test-Path $veyroExe) -and ($Features -match "local-llm")) {
        $portableStage = Join-Path $versionOut "_portable_stage"
        New-Item -ItemType Directory -Force -Path $portableStage | Out-Null
        Copy-Item $veyroExe $portableStage -Force
        Get-ChildItem $ReleaseDir -Filter "*.dll" |
            Where-Object { $_.Name -match '^(llama|ggml)' } |
            Copy-Item -Destination $portableStage -Force
        $zipPath = Join-Path $versionOut "Veyro_${Semver}_x64-portable.zip"
        if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
        Compress-Archive -Path (Join-Path $portableStage "*") -DestinationPath $zipPath -Force
        Remove-Item $portableStage -Recurse -Force
        Write-Host "Release portable: $zipPath"
    }

    $latestPointer = Join-Path (Join-Path $Root "release") "LATEST.txt"
    Set-Content -Path $latestPointer -Value $Semver -NoNewline
    Write-Host "Release folder: $versionOut"
}

$releaseDir = Join-Path $env:CARGO_TARGET_DIR "release"
$semver = Get-ProjectSemver -Root $repoRoot
Publish-ReleaseArtifacts -Root $repoRoot -ReleaseDir $releaseDir -Semver $semver -Features $features
