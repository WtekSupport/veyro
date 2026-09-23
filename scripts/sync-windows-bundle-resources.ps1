# Writes src-tauri/tauri.windows.conf.json from staged native DLLs in binaries/.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot
)

$ErrorActionPreference = "Stop"

$triple = "x86_64-pc-windows-msvc"
$binariesDir = Join-Path (Join-Path $RepoRoot "src-tauri") "binaries"
$destMap = [ordered]@{
    "llama-$triple.dll"                   = "llama.dll"
    "llama-common-$triple.dll"            = "llama-common.dll"
    "ggml-$triple.dll"                    = "ggml.dll"
    "ggml-base-$triple.dll"               = "ggml-base.dll"
    "ggml-cpu-$triple.dll"                = "ggml-cpu.dll"
    "ggml-vulkan-$triple.dll"             = "ggml-vulkan.dll"
    "sherpa-onnx-c-api-$triple.dll"       = "sherpa-onnx-c-api.dll"
    "sherpa-onnx-cxx-api-$triple.dll"     = "sherpa-onnx-cxx-api.dll"
    "onnxruntime-$triple.dll"             = "onnxruntime.dll"
    "onnxruntime_providers_shared-$triple.dll" = "onnxruntime_providers_shared.dll"
}

$resources = [ordered]@{
    "resources/sounds/*" = "sounds/"
}

foreach ($entry in $destMap.GetEnumerator()) {
    $source = Join-Path $binariesDir $entry.Key
    if (Test-Path $source) {
        $resources["binaries/$($entry.Key)"] = $entry.Value
    }
}

# libtorch/MKL must ship next to veyro.exe: the loader resolves c10.dll at process start
# (before Rust can download silero-te-runtime). Opt out only for dev experiments:
#   $env:VEYRO_SKIP_LIBTORCH_BUNDLE = "1"
$skipLibtorch = $env:VEYRO_SKIP_LIBTORCH_BUNDLE -eq "1"
# mkl*.dll use dots in the base name (mkl_core.1-...), not mkl_-...
$libtorchPattern = '^(c10|torch|torch_cpu|torch_global_deps|fbgemm|asmjit|fbjni|uv|libiomp5md|libiompstubs5md|pytorch_jni|mkl)'
if (-not $skipLibtorch) {
    foreach ($staged in Get-ChildItem $binariesDir -Filter "*-$triple.dll" -ErrorAction SilentlyContinue) {
        if ($staged.Name -notmatch $libtorchPattern) {
            continue
        }
        $destName = $staged.Name -replace "-$([regex]::Escape($triple))\.dll$", ".dll"
        $resources["binaries/$($staged.Name)"] = $destName
    }
} else {
    Write-Warning "Skipping libtorch/MKL in installer (VEYRO_SKIP_LIBTORCH_BUNDLE=1). Installed app will not start until DLLs are beside veyro.exe."
}

if (-not $skipLibtorch) {
    $c10Resource = "binaries/c10-$triple.dll"
    if (-not $resources.Contains($c10Resource)) {
        Write-Error "Missing staged $c10Resource - run scripts/stage-libtorch-dlls.ps1 before bundling (Silero TE / libtorch)."
    }
}

if ($resources.Count -le 1) {
    Write-Error "No native DLLs staged in $binariesDir. Run stage-llm-dlls.ps1 and/or stage-sherpa-dlls.ps1 first."
}

$config = [ordered]@{
    "`$schema" = "https://schema.tauri.app/config/2"
    bundle     = [ordered]@{
        resources = $resources
    }
}

$json = ($config | ConvertTo-Json -Depth 6 -Compress:$false)
$outPath = Join-Path (Join-Path $RepoRoot "src-tauri") "tauri.windows.conf.json"
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($outPath, $json, $utf8NoBom)
$dllCount = $resources.Count - 1
Write-Host "Updated $outPath with $dllCount bundled DLL resource entries."
