# Resolves local Whisper + local LLM Cargo features for this machine.
# Default: full local stack with the best GPU backend available.
# Opt-out env vars disable parts of the stack (see docs/BUILD.md).

. (Join-Path $PSScriptRoot "resolve-whisper-feature.ps1")

function Resolve-LocalLlmFeature {
    param(
        [Parameter(Mandatory = $true)]
        [string]$WhisperFeature
    )

    if ($env:VEYRO_DISABLE_LOCAL_LLM -eq "1") {
        Write-Host "Local LLM disabled (VEYRO_DISABLE_LOCAL_LLM=1)"
        return $null
    }

    if ($env:VEYRO_DISABLE_GPU -eq "1") {
        Write-Host "GPU disabled (VEYRO_DISABLE_GPU=1) - using CPU LLM"
        return "local-llm"
    }

    if ($env:VEYRO_LLM_FEATURE) {
        Write-Warning "VEYRO_LLM_FEATURE is deprecated; use VEYRO_DISABLE_LOCAL_LLM / VEYRO_DISABLE_GPU instead."
        Write-Host "Using VEYRO_LLM_FEATURE=$($env:VEYRO_LLM_FEATURE)"
        return $env:VEYRO_LLM_FEATURE
    }

    switch ($WhisperFeature) {
        "local-whisper-vulkan" { return "local-llm-vulkan" }
        "local-whisper-cuda" { return "local-llm-cuda" }
        default { return "local-llm" }
    }
}

function Get-CargoFeatureArgs {
    param(
        [AllowEmptyString()]
        [string]$Features
    )

    $args = @("--no-default-features")
    if ($Features) {
        $args += @("--features", $Features)
    }
    return $args
}

function Resolve-LocalFeatures {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    $parts = @()

    $whisper = Resolve-LocalWhisperFeature -RepoRoot $RepoRoot
    if ($whisper) {
        $parts += $whisper
    }

    $llm = Resolve-LocalLlmFeature -WhisperFeature $(if ($whisper) { $whisper } else { "local-whisper-vulkan" })
    if ($llm) {
        $parts += $llm
    }

    if ($env:VEYRO_DISABLE_SHERPA_STT -ne "1") {
        $parts += "local-sherpa-stt"
        if ($IsWindows -and $env:VEYRO_DISABLE_GPU -ne "1") {
            $parts += "local-sherpa-directml"
        }
    }

    if ($env:VEYRO_DISABLE_VAD_SILERO -ne "1") {
        $parts += "vad-silero"
        if ($env:VEYRO_DISABLE_SILERO_TE -ne "1") {
            $parts += "silero-te"
        }
    }

    if ($env:VEYRO_DISABLE_SEPARATION -ne "1") {
        $parts += "local-separation"
        if ($IsWindows -and $env:VEYRO_DISABLE_GPU -ne "1") {
            $parts += "local-separation-directml"
        }
    }

    if ($env:VEYRO_DISABLE_DIARIZATION -ne "1") {
        $parts += "local-diarization"
        if ($env:VEYRO_DISABLE_DIARIZATION_SPEAKRS -ne "1") {
            $parts += "local-diarization-speakrs"
        }
    }

    if ($parts.Count -eq 0) {
        Write-Warning "All local features disabled - building cloud-only (OpenAI) stack."
        return ""
    }

    $joined = $parts -join ","
    if ($joined -match "vulkan" -and -not $env:VULKAN_SDK) {
        . (Join-Path $PSScriptRoot "ensure-vulkan-sdk.ps1")
        $vulkanSdk = Get-VulkanSdkPath -RepoRoot $RepoRoot
        if (-not $vulkanSdk) {
            Write-Error @"
Vulkan Cargo features are enabled ($joined) but VULKAN_SDK is not set.
Install the Vulkan SDK, set VULKAN_SDK, or place a portable SDK under .tools/vulkan (see docs/BUILD.md).
"@
        }
        $env:VULKAN_SDK = $vulkanSdk
    }

    return $joined
}

# llama-cpp-sys builds ggml-vulkan via ExternalProject; parallel MSBuild races vulkan-shaders-gen.
# Rust compilation uses all logical cores; cmake --build stays serial (wrapper + VEYRO_CMAKE_PARALLEL).
function Get-CargoBuildJobs {
    if ($env:VEYRO_CARGO_JOBS) {
        return $env:VEYRO_CARGO_JOBS
    }
    $cores = [Environment]::ProcessorCount
    if ($cores -lt 1) {
        return "4"
    }
    return [string]$cores
}

function Get-CmakeBuildParallelism {
    if ($env:VEYRO_CMAKE_PARALLEL) {
        return $env:VEYRO_CMAKE_PARALLEL
    }
    return "1"
}

function Get-LlamaCppBuildParallelism {
    return Get-CargoBuildJobs
}

function Get-VeyroReleaseLayout {
    $profile = if ($env:VEYRO_RELEASE_PROFILE) {
        $env:VEYRO_RELEASE_PROFILE.Trim()
    } else {
        "release"
    }
    if (-not $profile) {
        $profile = "release"
    }

    if ($profile -eq "release") {
        return @{
            ProfileName  = "release"
            OutputDirName = "release"
            CargoArgs    = @("--release")
        }
    }

    return @{
        ProfileName   = $profile
        OutputDirName = $profile
        CargoArgs     = @("--profile", $profile)
    }
}

function Set-LlamaCppBuildParallelism {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    . (Join-Path $PSScriptRoot "enable-serial-cmake-wrapper.ps1")
    Enable-SerialCmakeWrapper -RepoRoot $RepoRoot

    $cargoJobs = Get-CargoBuildJobs
    $cmakeParallel = Get-CmakeBuildParallelism
    $env:CMAKE_BUILD_PARALLEL_LEVEL = $cmakeParallel
    $env:NUM_JOBS = $cmakeParallel
    $env:MSBUILDDISABLENODREUSE = "1"
    Write-Host "Build parallelism: cargo -j $cargoJobs, cmake --parallel $cmakeParallel (llama MSBuild serial)"
    return $cargoJobs
}

function Enable-DiarizationBlasLink {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [AllowEmptyString()]
        [string]$Features,
        [ValidateSet("debug", "release", "release-dist")]
        [string]$CargoProfile = "debug"
    )

    if ($Features -notmatch "local-diarization-speakrs") {
        return
    }
    if (-not ($IsWindows -or $env:OS -like "*Windows*")) {
        return
    }

    & (Join-Path $PSScriptRoot "set-diarization-blas-rustflags.ps1") -RepoRoot $RepoRoot -CargoProfile $CargoProfile
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}
