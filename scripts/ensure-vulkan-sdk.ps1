# Resolves VULKAN_SDK for cross-vendor GPU builds (NVIDIA / AMD / Intel).
# Returns the SDK root path, or $null if unavailable.

function Test-VulkanSdkRoot {
    param([string]$Root)

    if (-not $Root -or -not (Test-Path $Root)) {
        return $false
    }

    return Test-Path (Join-Path $Root "Include\vulkan\vulkan.h")
}

function Get-VulkanSdkPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    if (Test-VulkanSdkRoot $env:VULKAN_SDK) {
        return $env:VULKAN_SDK
    }

    $portable = Join-Path $RepoRoot ".tools\vulkan"
    if (Test-VulkanSdkRoot $portable) {
        return $portable
    }

    if (Test-Path "C:\VulkanSDK") {
        $latest = Get-ChildItem "C:\VulkanSDK" -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            Select-Object -First 1
        if ($latest -and (Test-VulkanSdkRoot $latest.FullName)) {
            return $latest.FullName
        }
    }

    $installer = Join-Path $RepoRoot ".tools\vulkan_sdk.exe"
    if (Test-Path $installer) {
        Write-Host "Portable Vulkan SDK not found - bootstrapping to $portable ..."
        New-Item -ItemType Directory -Force -Path $portable | Out-Null
        & $installer --root $portable install copy_only=1 --accept-licenses --default-answer --confirm-command | Out-Null
        if (Test-VulkanSdkRoot $portable) {
            return $portable
        }
    }

    return $null
}

function Write-GpuInventory {
    $controllers = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue
    if (-not $controllers) {
        Write-Host "GPU inventory: unavailable"
        return
    }

    foreach ($gpu in $controllers) {
        if ($gpu.Name) {
            Write-Host "Detected GPU: $($gpu.Name)"
        }
    }
}
