# Ensures Intel MKL is on the linker search path before Rust builds polyvoice/speakrs.
# speakrs turns on ndarray BLAS; feature unification makes polyvoice need cblas_* too.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,
    [ValidateSet("debug", "release", "release-dist")]
    [string]$CargoProfile = "debug"
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
. (Join-Path $PSScriptRoot "ensure-rust-path.ps1")

$helperManifest = Join-Path $RepoRoot "crates\veyro-diarization-blas\Cargo.toml"
if (-not (Test-Path $helperManifest)) {
    Write-Error "Missing $helperManifest"
}

Push-Location (Join-Path $RepoRoot "crates\veyro-diarization-blas")
try {
    $buildArgs = @("build", "-q", "--target", "x86_64-pc-windows-msvc")
    if ($CargoProfile -ne "debug") {
        $buildArgs += "--release"
    }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

$buildRoot = Join-Path $env:CARGO_TARGET_DIR "$CargoProfile\build"

$outputFile = Get-ChildItem -Path $buildRoot -Directory -Filter "intel-mkl-src-*" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    ForEach-Object { Join-Path $_.FullName "output" } |
    Where-Object { Test-Path -LiteralPath $_ } |
    Select-Object -First 1

$mklLibDir = $null
if ($outputFile) {
    foreach ($line in [System.IO.File]::ReadAllLines($outputFile)) {
        if ($line -like "cargo:rustc-link-search=*") {
            $mklLibDir = $line.Substring("cargo:rustc-link-search=".Length).Trim()
            if ($mklLibDir.StartsWith("native=")) {
                $mklLibDir = $mklLibDir.Substring("native=".Length)
            }
            break
        }
    }
}

if (-not $mklLibDir) {
    $fallback = Join-Path $env:APPDATA "ocipkg\ocipkg\data\ghcr.io\rust-math\rust-mkl\windows\mkl-static-lp64-seq"
    if (Test-Path -LiteralPath $fallback) {
        $mklLibDir = (Get-ChildItem -LiteralPath $fallback -Directory | Sort-Object Name -Descending | Select-Object -First 1).FullName
    }
}

if (-not $mklLibDir -or -not (Test-Path -LiteralPath $mklLibDir)) {
    Write-Warning "Intel MKL static lib dir not found (build output or ocipkg cache). Diarization link may fail with cblas_dgemm."
    return
}

# Same libs intel-mkl-src emits for mkl-static-lp64-seq (speakrs x86_64 default).
$linkDir = $mklLibDir
try {
    $fso = New-Object -ComObject Scripting.FileSystemObject
    $short = $fso.GetFolder($mklLibDir).ShortPath
    if ($short) {
        $linkDir = $short
    }
} catch {
    Write-Verbose "Short path unavailable for MKL dir, using long path (may need CARGO_ENCODED_RUSTFLAGS)."
}

# MSVC: pass MKL only at link time so proc-macros and other host tools are not forced to link BLAS.
$newFlags = @(
    "-C", "link-arg=/LIBPATH:$linkDir",
    "-C", "link-arg=mkl_intel_lp64.lib",
    "-C", "link-arg=mkl_core.lib",
    "-C", "link-arg=mkl_sequential.lib"
) -join " "
if ($env:RUSTFLAGS) {
    $env:RUSTFLAGS = "$newFlags $($env:RUSTFLAGS)"
} else {
    $env:RUSTFLAGS = $newFlags
}
Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue

if ($env:VEYRO_DEV_VERBOSE -eq "1") {
    Write-Host "Diarization BLAS: MKL $mklLibDir | RUSTFLAGS=$newFlags"
}
