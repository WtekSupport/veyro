# Stage llama.cpp runtime DLLs for Tauri externalBin bundling (Windows).

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [switch]$Required
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
$targetRoot = $env:CARGO_TARGET_DIR
$releaseDir = Join-Path $targetRoot "release"
$binariesDir = Join-Path (Join-Path $RepoRoot "src-tauri") "binaries"
$triple = "x86_64-pc-windows-msvc"

if (-not (Test-Path $releaseDir)) {
    if ($Required) {
        Write-Error "Release directory not found: $releaseDir"
    }
    return
}

New-Item -ItemType Directory -Force -Path $binariesDir | Out-Null

Get-ChildItem $binariesDir -Filter "*-$triple.dll" -ErrorAction SilentlyContinue | Remove-Item -Force

$copied = 0
foreach ($dll in Get-ChildItem $releaseDir -Filter "*.dll" | Where-Object { $_.Name -match '^(llama|ggml)' }) {
    $dest = Join-Path $binariesDir "$($dll.BaseName)-$triple.dll"
    Copy-Item $dll.FullName $dest -Force
    Write-Host "Staged $($dll.Name) -> $(Split-Path $dest -Leaf)"
    $copied++
}

if ($Required -and $copied -eq 0) {
    Write-Error "No llama/ggml DLLs found in $releaseDir"
}
