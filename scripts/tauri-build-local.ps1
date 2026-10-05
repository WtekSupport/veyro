# Release build with local Whisper (whisper-rs). Ensures CMake is available first.

param(

    [switch]$SkipVersionBump,

    [switch]$PublishProfile

)



$ErrorActionPreference = "Stop"



$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

Set-Location $repoRoot



if ($PublishProfile) {

    $env:VEYRO_RELEASE_PROFILE = "release-dist"

}



if (-not $SkipVersionBump) {

    & node (Join-Path $PSScriptRoot "bump-version.mjs") prod

    if ($LASTEXITCODE -ne 0) {

        exit $LASTEXITCODE

    }

} else {

    & node (Join-Path $PSScriptRoot "sync-version.mjs")

    if ($LASTEXITCODE -ne 0) {

        exit $LASTEXITCODE

    }

    Write-Host "Release build: keeping version from version.json (SkipVersionBump)."

}



. (Join-Path $PSScriptRoot "updater-signing-env.ps1")

& (Join-Path $PSScriptRoot "ensure-updater-keys.ps1")

if ($LASTEXITCODE -ne 0) {

    exit $LASTEXITCODE

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

. (Join-Path $PSScriptRoot "invoke-tauri-bundle.ps1")



$features = Resolve-LocalFeatures -RepoRoot $repoRoot

$cargoJobs = Set-LlamaCppBuildParallelism -RepoRoot $repoRoot

$cmakeParallel = Get-CmakeBuildParallelism

$releaseLayout = Get-VeyroReleaseLayout

$releaseOutputName = $releaseLayout.OutputDirName

$releaseCargoArgs = $releaseLayout.CargoArgs

$diarizationProfile = $releaseLayout.ProfileName

$releaseDir = Join-Path $env:CARGO_TARGET_DIR $releaseOutputName



Write-Host "Release Cargo profile: $($releaseLayout.ProfileName) (output: $releaseOutputName; publish profile: VEYRO_RELEASE_PROFILE=release-dist)"



$featureArgs = Get-CargoFeatureArgs -Features $features

Enable-DiarizationBlasLink -RepoRoot $repoRoot -Features $features -CargoProfile $diarizationProfile

# Frontend must exist before `cargo build`: tauri-build embeds `dist` at compile time.
Write-Host "Building frontend (required before Rust release compile)..."
Invoke-VeyroFrontendBuild



function Invoke-ReleaseBuild {

    Push-Location (Join-Path $repoRoot "src-tauri")

    try {

        cargo build @releaseCargoArgs @featureArgs -j $cargoJobs

        return $LASTEXITCODE

    } finally {

        Pop-Location

    }

}



Write-Host "Building Rust release with '$features' (output: $env:CARGO_TARGET_DIR\$releaseOutputName)..."

$buildExit = Invoke-ReleaseBuild



if ($buildExit -ne 0 -and $features -match "local-llm") {

    Write-Warning "Initial cargo build failed ($buildExit) - finishing llama.cpp cmake install and retrying..."

    & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Profile $releaseOutputName -Parallel $cmakeParallel

    $buildExit = Invoke-ReleaseBuild

}



if ($buildExit -ne 0) {

    exit $buildExit

}



$syncWindowsBundle = $false

if ($features -match "local-llm") {

    & (Join-Path $PSScriptRoot "finish-llama-cpp-build.ps1") -RepoRoot $repoRoot -Profile $releaseOutputName -Parallel $cmakeParallel

    & (Join-Path $PSScriptRoot "stage-llm-dlls.ps1") -RepoRoot $repoRoot -OutputDirName $releaseOutputName -Required

    $syncWindowsBundle = $true

}

if ($features -match "local-sherpa-stt") {

    & (Join-Path $PSScriptRoot "stage-sherpa-dlls.ps1") -RepoRoot $repoRoot -OutputDirName $releaseOutputName -Required

    $syncWindowsBundle = $true

}

if ($features -match "local-separation") {

    & (Join-Path $PSScriptRoot "stage-sherpa-dlls.ps1") -RepoRoot $repoRoot -OutputDirName $releaseOutputName

    $syncWindowsBundle = $true

}

if ($features -match "silero-te") {

    & (Join-Path $PSScriptRoot "stage-libtorch-dlls.ps1") -RepoRoot $repoRoot -Profile $releaseOutputName -Required

    $syncWindowsBundle = $true

}

if ($syncWindowsBundle) {

    & (Join-Path $PSScriptRoot "sync-windows-bundle-resources.ps1") -RepoRoot $repoRoot

}



$bundleArtifactDir = Join-Path $env:CARGO_TARGET_DIR "release"

if ($releaseOutputName -ne "release") {

    New-Item -ItemType Directory -Force -Path $bundleArtifactDir | Out-Null

    $builtExe = Join-Path $releaseDir "veyro.exe"

    if (-not (Test-Path -LiteralPath $builtExe)) {

        Write-Error "Missing release binary: $builtExe"

    }

    Copy-Item -LiteralPath $builtExe -Destination (Join-Path $bundleArtifactDir "veyro.exe") -Force

    Get-ChildItem -LiteralPath $releaseDir -Filter "*.dll" -ErrorAction SilentlyContinue |

        Copy-Item -Destination $bundleArtifactDir -Force

    Write-Host "Copied $($releaseLayout.ProfileName) artifacts into release/ for tauri bundle."

}

Write-Host "Bundling NSIS (tauri bundle)..."

if (-not (Set-UpdaterSigningEnv)) {

    Write-Error "Updater signing env missing before bundle. Run scripts/ensure-updater-keys.ps1"

}

Invoke-VeyroTauriBundle -Features $features

$builtExe = Join-Path $bundleArtifactDir "veyro.exe"
if (-not (Test-Path -LiteralPath $builtExe)) {
    Write-Error "Missing release binary: $builtExe"
}
$embedCheck = & findstr /M /C:"init.html" $builtExe 2>$null
if (-not $embedCheck) {
    Write-Error "Release veyro.exe does not contain embedded UI (init.html). Aborting."
}
$devUrlCheck = & findstr /M /C:"localhost:1420" $builtExe 2>$null
if ($devUrlCheck) {
    Write-Error "Release binary still contains localhost:1420 (devUrl). Rebuild after removing devUrl from tauri.conf.json."
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

                Write-Warning "No .sig file for updater (run ensure-updater-keys.ps1; signing env is applied before bundle)."

            }

        } else {

            Write-Warning "No NSIS setup.exe found under $nsisDir"

        }

    }



    $veyroExe = Join-Path $ReleaseDir "veyro.exe"

    if (Test-Path $veyroExe) {

        $portableStage = Join-Path $versionOut "_portable_stage"

        New-Item -ItemType Directory -Force -Path $portableStage | Out-Null

        Copy-Item $veyroExe $portableStage -Force

        $skipLibtorch = $env:VEYRO_SKIP_LIBTORCH_BUNDLE -eq "1"

        Get-ChildItem $ReleaseDir -Filter "*.dll" |

            Where-Object {

                $name = $_.Name

                if ($name -match '^(llama|ggml|sherpa-onnx|onnxruntime)') {

                    return $true

                }

                if (-not $skipLibtorch -and $name -match '^(c10|torch|torch_cpu|torch_global_deps|fbgemm|asmjit|fbjni|uv|libiomp|mkl_|pytorch_jni)') {

                    return $true

                }

                return $false

            } |

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



$semver = Get-ProjectSemver -Root $repoRoot

Publish-ReleaseArtifacts -Root $repoRoot -ReleaseDir $bundleArtifactDir -Semver $semver -Features $features

