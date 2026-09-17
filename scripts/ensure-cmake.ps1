# Ensures CMake 3.20+ is available for whisper-rs (local Whisper builds).
# Uses a project-local portable copy under .tools/cmake when not in PATH.

$ErrorActionPreference = "Stop"

$cmakeVersion = "3.31.6"
$toolsDir = Join-Path (Join-Path $PSScriptRoot "..") ".tools"
$cmakeRoot = Join-Path $toolsDir "cmake"
$cmakeBin = Join-Path $cmakeRoot "bin"
$cmakeExe = Join-Path $cmakeBin "cmake.exe"

function Test-CmakeReady {
    if (Test-Path $cmakeExe) {
        $version = & $cmakeExe --version 2>$null | Select-Object -First 1
        if ($version -match "cmake version (\d+\.\d+)") {
            $majorMinor = [version]$Matches[1]
            if ($majorMinor -ge [version]"3.20") {
                return $true
            }
        }
    }
    return $false
}

# Prefer system CMake when already installed.
$systemCmake = Get-Command cmake -ErrorAction SilentlyContinue
if ($systemCmake) {
    $versionLine = & cmake --version 2>$null | Select-Object -First 1
    if ($versionLine -match "cmake version (\d+\.\d+)") {
        $majorMinor = [version]$Matches[1]
        if ($majorMinor -ge [version]"3.20") {
            Write-Host "Using system CMake: $versionLine"
            exit 0
        }
    }
    Write-Warning "System cmake is too old ($versionLine). Installing portable CMake $cmakeVersion..."
}

if (Test-CmakeReady) {
    Write-Host "Using portable CMake at $cmakeBin"
    exit 0
}

Write-Host "Downloading portable CMake $cmakeVersion..."

New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
$zipName = "cmake-$cmakeVersion-windows-x86_64.zip"
$zipPath = Join-Path $toolsDir $zipName
$url = "https://github.com/Kitware/CMake/releases/download/v$cmakeVersion/$zipName"

if (-not (Test-Path $zipPath)) {
    Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing
}

$extractDir = Join-Path $toolsDir "cmake-extract"
if (Test-Path $extractDir) {
    Remove-Item -Recurse -Force $extractDir
}
New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

$extracted = Get-ChildItem -Path $extractDir -Directory | Select-Object -First 1
if (-not $extracted) {
    throw "CMake archive did not contain an expected folder."
}

if (Test-Path $cmakeRoot) {
    Remove-Item -Recurse -Force $cmakeRoot
}
Move-Item -Path $extracted.FullName -Destination $cmakeRoot
Remove-Item -Recurse -Force $extractDir

if (-not (Test-CmakeReady)) {
    throw "Portable CMake install failed at $cmakeExe"
}

Write-Host "Portable CMake ready: $(& $cmakeExe --version | Select-Object -First 1)"
