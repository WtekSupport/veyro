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

    if ($parts.Count -eq 0) {
        Write-Warning "All local features disabled - building cloud-only (OpenAI) stack."
        return ""
    }

    return ($parts -join ",")
}

# llama-cpp-sys builds ggml-vulkan via ExternalProject; high MSBuild parallelism
# races vulkan-shaders-gen configure/build/install (cmake cache / cmake_install.cmake).
# Cargo also forwards its -j value to build scripts as NUM_JOBS (cmake --build --parallel).
function Get-LlamaCppBuildParallelism {
    if ($env:VEYRO_CMAKE_PARALLEL) {
        return $env:VEYRO_CMAKE_PARALLEL
    }
    return "1"
}

function Set-LlamaCppBuildParallelism {
    $parallel = Get-LlamaCppBuildParallelism
    $env:CMAKE_BUILD_PARALLEL_LEVEL = $parallel
    $env:NUM_JOBS = $parallel
    Write-Host "CMake/Cargo parallel jobs: $parallel (override with VEYRO_CMAKE_PARALLEL)"
    return $parallel
}
