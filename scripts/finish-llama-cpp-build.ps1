# Completes a partial llama-cpp-sys cmake install (vulkan-shaders-gen race on Windows).
# Safe to run anytime; no-op when llama.cpp is already installed.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [string]$Profile = "debug",

    [string]$Parallel = "1"
)

$ErrorActionPreference = "Stop"
$script:LastLlamaCmakeExit = 1

. (Join-Path $PSScriptRoot "enable-serial-cmake-wrapper.ps1")
Enable-SerialCmakeWrapper -RepoRoot $RepoRoot

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
$targetDir = $env:CARGO_TARGET_DIR
$buildRoot = Join-Path $targetDir "$Profile\build"
if (-not (Test-Path $buildRoot)) {
    return
}

function Remove-LlamaCppOutTree {
    param([string]$LlamaSysDir)
    $outDir = Join-Path $LlamaSysDir "out"
    if (Test-Path $outDir) {
        Remove-Item -LiteralPath $outDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Clear-VulkanShadersGenPrefix {
    param([string]$CmakeBuild)
    $prefix = Join-Path $CmakeBuild "ggml\src\ggml-vulkan\vulkan-shaders-gen-prefix"
    if (Test-Path $prefix) {
        Remove-Item -LiteralPath $prefix -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-LlamaCppCmakeTarget {
    param(
        [string]$CmakeBuild,
        [string]$Target,
        [string]$Parallel
    )

    # Visual Studio generator: ExternalProject steps race unless MSBuild is single-process.
    $previousEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $buildOutput = & cmake --build $CmakeBuild --target $Target --config Release --parallel $Parallel -- /m:1 /p:BuildInParallel=false 2>&1
        $script:LastLlamaCmakeExit = [int]$LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousEap
    }
    foreach ($line in $buildOutput) {
        if ($line -is [System.Management.Automation.ErrorRecord]) {
            Write-Warning $line.ToString()
        } else {
            Write-Host $line
        }
    }
}

function Complete-LlamaCppInstall {
    param(
        [string]$CmakeBuild,
        [string]$Parallel,
        [string]$DirName
    )

    Write-Host "Finishing llama.cpp in $DirName (parallel=$Parallel, serial MSBuild)..."

    Invoke-LlamaCppCmakeTarget -CmakeBuild $CmakeBuild -Target "vulkan-shaders-gen" -Parallel $Parallel
    $shaderExit = $script:LastLlamaCmakeExit
    if ($shaderExit -ne 0) {
        Write-Host "vulkan-shaders-gen failed (exit $shaderExit) - clearing prefix and retrying once..."
        Clear-VulkanShadersGenPrefix -CmakeBuild $CmakeBuild
        Invoke-LlamaCppCmakeTarget -CmakeBuild $CmakeBuild -Target "vulkan-shaders-gen" -Parallel $Parallel
        $shaderExit = $script:LastLlamaCmakeExit
        if ($shaderExit -ne 0) {
            return $shaderExit
        }
    }

    Invoke-LlamaCppCmakeTarget -CmakeBuild $CmakeBuild -Target "install" -Parallel $Parallel
    $installExit = $script:LastLlamaCmakeExit
    if ($installExit -ne 0) {
        Write-Host "llama.cpp install failed (exit $installExit) - clearing vulkan-shaders-gen prefix and retrying install..."
        Clear-VulkanShadersGenPrefix -CmakeBuild $CmakeBuild
        Invoke-LlamaCppCmakeTarget -CmakeBuild $CmakeBuild -Target "vulkan-shaders-gen" -Parallel $Parallel
        Invoke-LlamaCppCmakeTarget -CmakeBuild $CmakeBuild -Target "install" -Parallel $Parallel
        $installExit = $script:LastLlamaCmakeExit
    }

    return $installExit
}

$llamaDirs = Get-ChildItem $buildRoot -Directory -Filter "llama-cpp-sys-2-*" -ErrorAction SilentlyContinue
foreach ($dir in $llamaDirs) {
    $cmakeBuild = Join-Path $dir.FullName "out\build"
    $installedMarker = Join-Path $dir.FullName "out\bin\llama.dll"
    if (Test-Path $installedMarker) {
        continue
    }
    if (-not (Test-Path $cmakeBuild)) {
        continue
    }

    $cmakeCache = Join-Path $cmakeBuild "CMakeCache.txt"
    if (-not (Test-Path $cmakeCache)) {
        Write-Host "llama.cpp cmake cache missing in $($dir.Name) - removing stale out/ (cargo will reconfigure)."
        Remove-LlamaCppOutTree -LlamaSysDir $dir.FullName
        continue
    }

    $exit = Complete-LlamaCppInstall -CmakeBuild $cmakeBuild -Parallel $Parallel -DirName $dir.Name
    if ($exit -ne 0) {
        Write-Host "llama.cpp cmake install still failed (exit $exit). Removing out/ so the next cargo build can reconfigure."
        Write-Host "Tip: keep VEYRO_CMAKE_PARALLEL=1 (default) and re-run dev; the second attempt often succeeds after finish."
        Remove-LlamaCppOutTree -LlamaSysDir $dir.FullName
    }
}
