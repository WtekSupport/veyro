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

if ($IsWindows -or $env:OS -like "*Windows*") {
    & (Join-Path $PSScriptRoot "ensure-intel-mkl-redist.ps1") -RepoRoot $RepoRoot
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

$stagedLibtorchPattern = '^(c10|torch|torch_cpu|torch_global_deps|fbgemm|asmjit|fbjni|uv|libiomp5md|libiompstubs5md|mkl_|pytorch_jni)-'
Get-ChildItem $binariesDir -Filter "*-$triple.dll" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match $stagedLibtorchPattern } |
    Remove-Item -Force

$mklDispatchDir = Join-Path $RepoRoot ".tools\mkl-dispatch"

$depsDir = Join-Path $profileDir "deps"
New-Item -ItemType Directory -Force -Path $depsDir | Out-Null

function Stage-LibtorchDll {
    param([string]$SrcPath)
    $name = [IO.Path]::GetFileName($SrcPath)
    $exeDest = Join-Path $profileDir $name
    Copy-Item $SrcPath $exeDest -Force
    Copy-Item $SrcPath (Join-Path $depsDir $name) -Force
    $base = [IO.Path]::GetFileNameWithoutExtension($name)
    $bundleDest = Join-Path $binariesDir "$base-$triple.dll"
    Copy-Item $SrcPath $bundleDest -Force
    Write-Host "Staged libtorch $name -> $Profile (+ deps/) + binaries/"
}

$copied = 0
foreach ($dll in Get-ChildItem $libDir -Filter "*.dll") {
    Stage-LibtorchDll $dll.FullName
    $copied++
}

if (Test-Path $mklDispatchDir) {
    foreach ($dll in Get-ChildItem $mklDispatchDir -Filter "mkl*.dll" -File -ErrorAction SilentlyContinue) {
        $exeDest = Join-Path $profileDir $dll.Name
        if (-not (Test-Path $exeDest)) {
            Stage-LibtorchDll $dll.FullName
            $copied++
        }
    }
}

if ($Required -and $copied -eq 0) {
    Write-Error "No libtorch DLLs under $libDir"
}
