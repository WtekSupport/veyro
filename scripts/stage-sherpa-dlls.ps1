# Stage sherpa-onnx runtime DLLs for Tauri bundle resources (Windows, shared feature).

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [switch]$Required
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
$releaseDir = Join-Path $env:CARGO_TARGET_DIR "release"
$binariesDir = Join-Path (Join-Path $RepoRoot "src-tauri") "binaries"
$triple = "x86_64-pc-windows-msvc"

if (-not (Test-Path $releaseDir)) {
    if ($Required) {
        Write-Error "Release directory not found: $releaseDir"
    }
    return
}

New-Item -ItemType Directory -Force -Path $binariesDir | Out-Null

Get-ChildItem $binariesDir -Filter "*-$triple.dll" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^(sherpa-onnx|onnxruntime)' } |
    Remove-Item -Force

$patterns = @(
    "^sherpa-onnx-",
    "^onnxruntime"
)

$copied = 0
foreach ($dll in Get-ChildItem $releaseDir -Filter "*.dll") {
    $match = $false
    foreach ($pattern in $patterns) {
        if ($dll.Name -match $pattern) {
            $match = $true
            break
        }
    }
    if (-not $match) {
        continue
    }
    $dest = Join-Path $binariesDir "$($dll.BaseName)-$triple.dll"
    Copy-Item $dll.FullName $dest -Force
    Write-Host "Staged $($dll.Name) -> $(Split-Path $dest -Leaf)"
    $copied++
}

if ($Required -and $copied -eq 0) {
    Write-Error "No sherpa/onnxruntime DLLs found in $releaseDir (build with local-sherpa-stt first)."
}
