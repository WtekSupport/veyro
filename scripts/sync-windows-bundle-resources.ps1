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

$bundleLibtorch = $env:VEYRO_BUNDLE_LIBTORCH -eq "1"
# mkl*.dll use dots in the base name (mkl_core.1-…), not mkl_-…
$libtorchPattern = '^(c10|torch|torch_cpu|torch_global_deps|fbgemm|asmjit|fbjni|uv|libiomp5md|libiompstubs5md|pytorch_jni|mkl)'
if ($bundleLibtorch) {
    foreach ($staged in Get-ChildItem $binariesDir -Filter "*-$triple.dll" -ErrorAction SilentlyContinue) {
        if ($staged.Name -notmatch $libtorchPattern) {
            continue
        }
        $destName = $staged.Name -replace "-$([regex]::Escape($triple))\.dll$", ".dll"
        $resources["binaries/$($staged.Name)"] = $destName
    }
} else {
    Write-Host "Skipping libtorch/MKL DLLs in installer bundle (on-demand silero-te-runtime-v1). Set VEYRO_BUNDLE_LIBTORCH=1 to bundle."
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
Write-Host "Updated $outPath with $($resources.Count - 1) DLL resource(s)."
