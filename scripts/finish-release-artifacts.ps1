# Copy NSIS bundle from CARGO_TARGET_DIR into release/<semver>/ (sig + latest.json).
param(
    [Parameter(Mandatory = $true)]
    [string]$Semver
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "ensure-cargo-target.ps1")

$releaseDir = Join-Path $env:CARGO_TARGET_DIR "release"
$versionOut = Join-Path (Join-Path $repoRoot "release") $Semver
New-Item -ItemType Directory -Force -Path $versionOut | Out-Null

$nsisDir = Join-Path (Join-Path $releaseDir "bundle") "nsis"
$setup = Get-ChildItem $nsisDir -Filter "Veyro_${Semver}_x64-setup.exe" -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $setup) {
    $setup = Get-ChildItem $nsisDir -Filter "*setup.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}
if (-not $setup) {
    Write-Error "No setup.exe under $nsisDir"
}

Copy-Item $setup.FullName (Join-Path $versionOut $setup.Name) -Force
Write-Host "Release installer: $(Join-Path $versionOut $setup.Name)"

$sigSource = "$($setup.FullName).sig"
if (Test-Path $sigSource) {
    Copy-Item $sigSource (Join-Path $versionOut "$($setup.Name).sig") -Force
    $signature = (Get-Content $sigSource -Raw).Trim()
    $downloadUrl =
        "https://github.com/WtekSupport/veyro/releases/download/v$Semver/$($setup.Name)"
    $manifest = [ordered]@{
        version   = $Semver
        notes     = "Veyro $Semver"
        pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        platforms = [ordered]@{
            "windows-x86_64" = [ordered]@{
                url       = $downloadUrl
                signature = $signature
            }
        }
    }
    $latestJson = Join-Path $versionOut "latest.json"
    $jsonText = ($manifest | ConvertTo-Json -Depth 6)
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($latestJson, $jsonText, $utf8NoBom)
    Write-Host "Release manifest: $latestJson"
} else {
    Write-Warning "No .sig file; run tauri build with updater signing env configured."
}

$veyroExe = Join-Path $releaseDir "veyro.exe"
if (Test-Path $veyroExe) {
    $portableStage = Join-Path $versionOut "_portable_stage"
    New-Item -ItemType Directory -Force -Path $portableStage | Out-Null
    Copy-Item $veyroExe $portableStage -Force
    Get-ChildItem $releaseDir -Filter "*.dll" |
        Where-Object { $_.Name -match '^(llama|ggml|sherpa-onnx|onnxruntime)' } |
        Copy-Item -Destination $portableStage -Force
    $zipPath = Join-Path $versionOut "Veyro_${Semver}_x64-portable.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path (Join-Path $portableStage "*") -DestinationPath $zipPath -Force
    Remove-Item $portableStage -Recurse -Force
    Write-Host "Release portable: $zipPath"
}

Set-Content -Path (Join-Path (Join-Path $repoRoot "release") "LATEST.txt") -Value $Semver -NoNewline
Write-Host "Done: release/$Semver/"
