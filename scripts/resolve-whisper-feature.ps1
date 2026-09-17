# Picks the best local-whisper Cargo feature for this machine.
# Default: Vulkan (NVIDIA/AMD/Intel) -> CUDA (opt-in) -> CPU fallback.
# Use VEYRO_DISABLE_* env vars to opt out (see docs/BUILD.md).

. (Join-Path $PSScriptRoot "ensure-vulkan-sdk.ps1")

function Resolve-LocalWhisperFeature {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    if ($env:VEYRO_DISABLE_LOCAL_WHISPER -eq "1") {
        Write-Host "Local Whisper disabled (VEYRO_DISABLE_LOCAL_WHISPER=1)"
        return $null
    }

    if ($env:VEYRO_DISABLE_GPU -eq "1") {
        Write-Host "GPU disabled (VEYRO_DISABLE_GPU=1) - using CPU Whisper"
        return "local-whisper"
    }

    if ($env:VEYRO_WHISPER_FEATURE) {
        $forced = $env:VEYRO_WHISPER_FEATURE
        Write-Warning "VEYRO_WHISPER_FEATURE is deprecated; prefer VEYRO_DISABLE_GPU=1 for CPU-only builds."
        Write-Host "Using VEYRO_WHISPER_FEATURE=$forced"
        if ($forced -eq "local-whisper") {
            $vulkanSdk = Get-VulkanSdkPath -RepoRoot $RepoRoot
            if ($vulkanSdk) {
                Write-Warning @"
VEYRO_WHISPER_FEATURE=local-whisper disables GPU acceleration.
Vulkan SDK is available at: $vulkanSdk
Remove the variable or set VEYRO_DISABLE_GPU=1 instead.
"@
            }
        }
        return $forced
    }

    Write-GpuInventory

    $vulkanSdk = Get-VulkanSdkPath -RepoRoot $RepoRoot
    if ($vulkanSdk) {
        $env:VULKAN_SDK = $vulkanSdk
        Write-Host "Vulkan SDK: $vulkanSdk"
        Write-Host "Selected GPU backend: Vulkan (NVIDIA / AMD / Intel)"
        return "local-whisper-vulkan"
    }

    if ($env:VEYRO_ALLOW_CUDA -eq "1") {
        $cudaRoot = $env:CUDA_PATH
        if ($cudaRoot -and (Test-Path $cudaRoot)) {
            $nvcc = Join-Path (Join-Path $cudaRoot "bin") "nvcc.exe"
            if (Test-Path $nvcc) {
                $cudaBin = Join-Path $cudaRoot "bin"
                if ($env:PATH -notlike "*$cudaBin*") {
                    $env:PATH = "${cudaBin};$($env:PATH)"
                }
                $env:CUDACXX = $nvcc
                $env:CMAKE_CUDA_COMPILER = $nvcc
                Write-Host "CUDA toolkit: $cudaRoot"
                Write-Host "Selected GPU backend: CUDA (NVIDIA only)"
                return "local-whisper-cuda"
            }
        }
    }

    Write-Host "No GPU SDK found - using CPU backend."
    Write-Host "Tip: Vulkan SDK enables GPU on NVIDIA and AMD. Place SDK in .tools/vulkan or set VULKAN_SDK."
    return "local-whisper"
}
