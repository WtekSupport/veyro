# Publish libtorch + MKL runtime DLLs for on-demand Silero TE (see runtime_store.rs).
param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,

    [string]$Repo = "WtekSupport/veyro",
    [string]$Tag = "silero-te-runtime-v1",
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

$triple = "x86_64-pc-windows-msvc"
$binariesDir = Join-Path (Join-Path $RepoRoot "src-tauri") "binaries"
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "C:\veyro-target" }
$releaseDir = Join-Path $targetRoot $Profile

$libtorchPattern = '^(c10|torch|torch_cpu|torch_global_deps|fbgemm|asmjit|fbjni|uv|libiomp5md|libiompstubs5md|pytorch_jni|mkl)'

function Get-RuntimeDllPaths {
    $paths = [System.Collections.Generic.List[string]]::new()
    if (Test-Path $releaseDir) {
        Get-ChildItem $releaseDir -Filter "*.dll" -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match $libtorchPattern } |
            ForEach-Object { $paths.Add($_.FullName) }
    }
    if ($paths.Count -eq 0 -and (Test-Path $binariesDir)) {
        Get-ChildItem $binariesDir -Filter "*-$triple.dll" -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match $libtorchPattern } |
            ForEach-Object { $paths.Add($_.FullName) }
    }
    return $paths
}

$dllPaths = @(Get-RuntimeDllPaths)
if ($dllPaths.Count -eq 0) {
    Write-Error "No libtorch/MKL DLLs found under $releaseDir or $binariesDir. Run a silero-te release build first."
}

$staging = Join-Path $env:TEMP "veyro-silero-te-runtime-pack"
if (Test-Path $staging) {
    Remove-Item -Recurse -Force $staging
}
New-Item -ItemType Directory -Force -Path $staging | Out-Null

$manifestFiles = @()
foreach ($src in $dllPaths) {
    $name = [IO.Path]::GetFileName($src)
    if ($name -match "-$([regex]::Escape($triple))\.dll$") {
        $name = $name -replace "-$([regex]::Escape($triple))\.dll$", ".dll"
    }
    $dest = Join-Path $staging $name
    Copy-Item $src $dest -Force
    $hash = (Get-FileHash -Path $dest -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestFiles += [ordered]@{
        name   = $name
        sha256 = $hash
    }
}

$manifest = [ordered]@{
    version = 1
    files   = $manifestFiles
}
$manifestPath = Join-Path $staging "runtime-manifest.json"
$manifestJson = ($manifest | ConvertTo-Json -Depth 4)
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($manifestPath, $manifestJson, $utf8NoBom)

$zipPath = Join-Path $staging "silero-te-runtime-win-x64.zip"
if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
$zipItems = Get-ChildItem $staging -File | Where-Object { $_.Name -ne "silero-te-runtime-win-x64.zip" }
Compress-Archive -Path ($zipItems | ForEach-Object { $_.FullName }) -DestinationPath $zipPath -Force

Write-Host "Staged $($manifestFiles.Count) DLL(s), zip: $zipPath"

$uploadPaths = @($manifestPath, $zipPath)

$ghCmd = Get-Command gh -ErrorAction SilentlyContinue
if ($ghCmd) {
    $gh = $ghCmd.Source
} elseif (Test-Path (Join-Path $RepoRoot ".tools\gh\gh.exe")) {
    $gh = Join-Path $RepoRoot ".tools\gh\gh.exe"
} else {
    $gh = $null
}

if (-not $gh) {
    Write-Host "Install GitHub CLI (gh) and run: gh auth login"
    Write-Host "Upload to release tag: $Tag"
    foreach ($path in $uploadPaths) {
        Write-Host "  - $path"
    }
    exit 0
}

$viewArgs = @("release", "view", $Tag, "--repo", $Repo)
$viewOut = & $gh @viewArgs 2>&1 | Out-String
$releaseMissing = $LASTEXITCODE -ne 0 -or ($viewOut -match "release not found")
if ($releaseMissing) {
    Write-Host "Creating prerelease $Tag on $Repo ..."
    & $gh release create $Tag @uploadPaths --repo $Repo --prerelease --title "Silero TE runtime (Windows x64)" --notes "On-demand libtorch + MKL DLLs for Silero TE in Veyro."
} else {
    Write-Host "Updating assets on release $Tag ..."
    & $gh release upload $Tag @uploadPaths --repo $Repo --clobber
}

if ($LASTEXITCODE -ne 0) {
    Write-Error "gh release upload failed ($LASTEXITCODE)"
}

Write-Host "Done. Verify: https://github.com/$Repo/releases/tag/$Tag"
