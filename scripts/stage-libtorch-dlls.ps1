# Stage libtorch runtime DLLs (Silero TE / tch) next to veyro.exe and for Tauri bundle resources.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [ValidateSet("debug", "release")]
    [string]$Profile = "release",

    [switch]$Required
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")

$targetRoot = $env:CARGO_TARGET_DIR
$profileDir = Join-Path $targetRoot $Profile
$buildRoot = Join-Path $profileDir "build"
$binariesDir = Join-Path (Join-Path $RepoRoot "src-tauri") "binaries"
$triple = "x86_64-pc-windows-msvc"

function Get-LibtorchLibDir {
    if (-not (Test-Path $buildRoot)) {
        return $null
    }
    $torchBuild = Get-ChildItem $buildRoot -Directory -Filter "torch-sys-*" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $torchBuild) {
        return $null
    }
    $libDir = Join-Path $torchBuild.FullName "out\libtorch\libtorch\lib"
    if (Test-Path $libDir) {
        return $libDir
    }
    return $null
}

$libDir = Get-LibtorchLibDir
if (-not $libDir) {
    if ($Required) {
        Write-Error "libtorch lib folder not found under $buildRoot (build with silero-te feature first)."
    }
    return
}

New-Item -ItemType Directory -Force -Path $binariesDir | Out-Null
New-Item -ItemType Directory -Force -Path $profileDir | Out-Null

$stagedLibtorchPattern = '^(c10|torch|torch_cpu|torch_global_deps|fbgemm|asmjit|fbjni|uv|libiomp5md|libiompstubs5md|mkl_core\.1|mkl_intel_thread\.1|pytorch_jni)-'
Get-ChildItem $binariesDir -Filter "*-$triple.dll" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match $stagedLibtorchPattern } |
    Remove-Item -Force

$copied = 0
foreach ($dll in Get-ChildItem $libDir -Filter "*.dll") {
    $exeDest = Join-Path $profileDir $dll.Name
    Copy-Item $dll.FullName $exeDest -Force

    $bundleDest = Join-Path $binariesDir "$($dll.BaseName)-$triple.dll"
    Copy-Item $dll.FullName $bundleDest -Force
    Write-Host "Staged libtorch $($dll.Name) -> $Profile + binaries/"
    $copied++
}

if ($Required -and $copied -eq 0) {
    Write-Error "No libtorch DLLs under $libDir"
}
