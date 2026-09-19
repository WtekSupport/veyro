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

    $batPath = Join-Path $wrapperDir "cmake.bat"
    $batContent = @'
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
echo.%* | findstr /I /C:" /m:" >nul
if not errorlevel 1 (
  "%REAL%" %*
  exit /b %ERRORLEVEL%
)
echo.%* | findstr /I /C:"-- /m:" >nul
if not errorlevel 1 (
  "%REAL%" %*
  exit /b %ERRORLEVEL%
)
"%REAL%" %* -- /m:1 /p:BuildInParallel=false
exit /b %ERRORLEVEL%
'@

    Set-Content -LiteralPath $batPath -Value $batContent -Encoding ASCII

    if ($env:PATH -notlike "*$wrapperDir*") {
        $env:PATH = "$wrapperDir;$env:PATH"
    }
}
