# Invoked by .tools/veyro-cmake-wrapper/cmake.bat for `cmake --build` only.
# Forces --parallel 1 so Cargo -j does not race llama.cpp vulkan-shaders-gen via NUM_JOBS.

param(
    [Parameter(Mandatory = $true)]
    [string]$RealCmake,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CmakeArgs
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $RealCmake)) {
    Write-Error "cmake not found at $RealCmake"
}

$out = [System.Collections.Generic.List[string]]::new()
$parallelSet = $false
$skipNext = $false

for ($i = 0; $i -lt $CmakeArgs.Count; $i++) {
    if ($skipNext) {
        $skipNext = $false
        continue
    }

    $arg = $CmakeArgs[$i]
    if ($arg -eq "--parallel" -or $arg -eq "-j") {
        $parallelSet = $true
        if ($i + 1 -lt $CmakeArgs.Count -and $CmakeArgs[$i + 1] -notmatch "^-") {
            $skipNext = $true
        }
        continue
    }
    if ($arg -match "^--parallel=") {
        $parallelSet = $true
        continue
    }

    $out.Add($arg) | Out-Null
}

if (-not $parallelSet) {
    $out.Add("--parallel") | Out-Null
}
$out.Add("1") | Out-Null

$hasMsbuildSerial = $false
foreach ($arg in $out) {
    if ($arg -match "/m:1" -or $arg -match "BuildInParallel=false") {
        $hasMsbuildSerial = $true
        break
    }
}
if (-not $hasMsbuildSerial) {
    $out.Add("--") | Out-Null
    $out.Add("/m:1") | Out-Null
    $out.Add("/p:BuildInParallel=false") | Out-Null
}

& $RealCmake @out
exit $LASTEXITCODE
