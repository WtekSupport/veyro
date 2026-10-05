# Prepends a cmake.bat shim that forces serial MSBuild (/m:1) for `cmake --build`.
# llama.cpp's vulkan-shaders-gen ExternalProject races under parallel MSBuild.

function Enable-SerialCmakeWrapper {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    $wrapperDir = Join-Path $RepoRoot ".tools\veyro-cmake-wrapper"
    if (-not (Test-Path $wrapperDir)) {
        New-Item -ItemType Directory -Force -Path $wrapperDir | Out-Null
    }

    $serialMjs = Join-Path $RepoRoot "scripts\cmake-build-serial.mjs"
    $batPath = Join-Path $wrapperDir "cmake.bat"
    # Cargo sets NUM_JOBS from `cargo -j`, so cmake-rs passes `--parallel N` unless we rewrite argv.
    $batContent = @"
@echo off
setlocal EnableExtensions
set "REAL=%~dp0..\cmake\bin\cmake.exe"
if not exist "%REAL%" (
  where cmake.exe >nul 2>&1
  if errorlevel 1 (
    echo cmake not found 1>&2
    exit /b 1
  )
  set "REAL=cmake.exe"
)
echo.%* | findstr /I /C:"--build" >nul
if errorlevel 1 (
  "%REAL%" %*
  exit /b %ERRORLEVEL%
)
set "CMAKE_BUILD_PARALLEL_LEVEL=1"
node "$serialMjs" "%REAL%" %*
exit /b %ERRORLEVEL%
"@

    Set-Content -LiteralPath $batPath -Value $batContent -Encoding ASCII

    if ($env:PATH -notlike "*$wrapperDir*") {
        $env:PATH = "$wrapperDir;$env:PATH"
    }
}
