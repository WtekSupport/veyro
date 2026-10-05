# Dev without bump-version / pre-cargo; still elevates before run (veyro.exe requireAdministrator).
$ErrorActionPreference = "Stop"

function Test-VeyroDevIsAdmin {
    if (-not (($IsWindows -eq $true) -or ($env:OS -like "*Windows*"))) {
        return $true
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location -LiteralPath $repoRoot

. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")
& (Join-Path $PSScriptRoot "ensure-cmake.ps1")
$cmakeBin = Join-Path (Join-Path (Join-Path $repoRoot ".tools") "cmake") "bin"
if (Test-Path (Join-Path $cmakeBin "cmake.exe")) {
    $env:PATH = "$cmakeBin;$env:PATH"
}
. (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$system32 = Join-Path $env:SystemRoot "System32"
$nodejsDir = Join-Path ${env:ProgramFiles} "nodejs"
foreach ($prefix in @($system32, $nodejsDir)) {
    if (Test-Path $prefix) {
        $env:PATH = "$prefix;$env:PATH"
    }
}

$features = Resolve-LocalFeatures -RepoRoot $repoRoot
$cargoJobs = Set-LlamaCppBuildParallelism -RepoRoot $repoRoot
Enable-DiarizationBlasLink -RepoRoot $repoRoot -Features $features
if ($null -eq $env:VEYRO_CARGO_INCREMENTAL) {
    $env:CARGO_INCREMENTAL = "0"
}

$cargoExe = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargoExe) {
    Write-Error "cargo not found in PATH."
}

$tauriJs = Join-Path $repoRoot "node_modules\@tauri-apps\cli\tauri.js"
if (-not (Test-Path $tauriJs)) {
    Write-Error "Run npm install first."
}

if ($features -match "silero-te") {
    & (Join-Path $PSScriptRoot "stage-libtorch-dlls.ps1") -RepoRoot $repoRoot -Profile "debug"
}

$featureArgs = Get-CargoFeatureArgs -Features $features

if (-not (Test-VeyroDevIsAdmin)) {
    Write-Host "veyro.exe must run elevated (Windows manifest). Relaunching this script with UAC..."
    . (Join-Path $PSScriptRoot "ensure-admin.ps1") -CallerScript $PSCommandPath
    if (-not (Test-VeyroDevIsAdmin)) {
        Write-Error "Elevation required to run veyro.exe (os error 740 without admin). Accept UAC or open PowerShell as Administrator."
        exit 1
    }
}

Write-Host "Direct dev: features=$features cargoJobs=$cargoJobs"
& node $tauriJs dev --features $features -- -j $cargoJobs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
