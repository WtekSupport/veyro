# Ensures Intel MKL 2021.4 runtime DLLs (dispatch + VML) for libtorch on Windows.
# libtorch from torch-sys often ships only mkl_core + mkl_intel_thread; missing DLLs cause fatal MKL errors at runtime.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot
)

$ErrorActionPreference = "Stop"

$mklVersion = "2021.4.0.640"
$toolsDir = Join-Path $RepoRoot ".tools"
$cacheRoot = Join-Path $toolsDir "intel-mkl-redist"
$nativeDir = Join-Path $cacheRoot "runtimes\win-x64\native"
$dispatchDir = Join-Path $RepoRoot ".tools\mkl-dispatch"

function Test-MklRedistReady {
    if (-not (Test-Path $nativeDir)) {
        return $false
    }
    $required = @("mkl_def.1.dll", "mkl_avx2.1.dll")
    foreach ($name in $required) {
        if (-not (Test-Path (Join-Path $nativeDir $name))) {
            return $false
        }
    }
    return $true
}

if (Test-MklRedistReady) {
    Write-Host "Intel MKL redist ready at $nativeDir"
} else {
    New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
    $nupkg = Join-Path $toolsDir "intelmkl.redist.win-x64.$mklVersion.nupkg"
    $url = "https://www.nuget.org/api/v2/package/intelmkl.redist.win-x64/$mklVersion"
    if (-not (Test-Path $nupkg)) {
        Write-Host "Downloading Intel MKL redist $mklVersion from NuGet..."
        Invoke-WebRequest -Uri $url -OutFile $nupkg -UseBasicParsing
    }
    if (Test-Path $cacheRoot) {
        Remove-Item -Recurse -Force $cacheRoot
    }
    New-Item -ItemType Directory -Force -Path $cacheRoot | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($nupkg, $cacheRoot)
    if (-not (Test-MklRedistReady)) {
        Write-Error "MKL redist extract failed or incomplete under $nativeDir"
    }
    Write-Host "Extracted Intel MKL redist to $nativeDir"
}

New-Item -ItemType Directory -Force -Path $dispatchDir | Out-Null
$mklDlls = @(Get-ChildItem $nativeDir -Filter "mkl*.dll" -File)
foreach ($dll in $mklDlls) {
    Copy-Item $dll.FullName (Join-Path $dispatchDir $dll.Name) -Force
}
Write-Host "Synced $($mklDlls.Count) MKL DLL(s) to $dispatchDir"
