# Completes a partial llama-cpp-sys cmake install (vulkan-shaders-gen race on Windows).
# Safe to run anytime; no-op when llama.cpp is already installed.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [string]$Profile = "debug",

    [string]$Parallel = "1"
)

$ErrorActionPreference = "Stop"

$cmakeBin = Join-Path (Join-Path (Join-Path $RepoRoot ".tools") "cmake") "bin"
if (Test-Path (Join-Path $cmakeBin "cmake.exe")) {
    $env:PATH = "$cmakeBin;$env:PATH"
}

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
$targetDir = $env:CARGO_TARGET_DIR
$buildRoot = Join-Path $targetDir "$Profile\build"
if (-not (Test-Path $buildRoot)) {
    return
}

$llamaDirs = Get-ChildItem $buildRoot -Directory -Filter "llama-cpp-sys-2-*" -ErrorAction SilentlyContinue
foreach ($dir in $llamaDirs) {
    $cmakeBuild = Join-Path $dir.FullName "out\build"
    $installedMarker = Join-Path $dir.FullName "out\bin\llama.dll"
    if (-not (Test-Path $cmakeBuild)) {
        continue
    }
    if (Test-Path $installedMarker) {
        continue
    }

    Write-Host "Finishing llama.cpp cmake install in $($dir.Name) (parallel=$Parallel)..."
    & cmake --build $cmakeBuild --target install --config Release --parallel $Parallel
    if ($LASTEXITCODE -ne 0) {
        Write-Error "llama.cpp cmake install failed (exit $LASTEXITCODE). Retry with VEYRO_CMAKE_PARALLEL=1"
    }
}
