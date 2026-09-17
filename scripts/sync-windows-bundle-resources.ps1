# Writes src-tauri/tauri.windows.conf.json from staged llama/ggml DLLs in binaries/.

param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot
)

$ErrorActionPreference = "Stop"

$triple = "x86_64-pc-windows-msvc"
$binariesDir = Join-Path (Join-Path $RepoRoot "src-tauri") "binaries"
$destMap = [ordered]@{
    "llama-$triple.dll"        = "llama.dll"
    "llama-common-$triple.dll" = "llama-common.dll"
    "ggml-$triple.dll"         = "ggml.dll"
    "ggml-base-$triple.dll"    = "ggml-base.dll"
    "ggml-cpu-$triple.dll"     = "ggml-cpu.dll"
    "ggml-vulkan-$triple.dll"  = "ggml-vulkan.dll"
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

if ($resources.Count -le 1) {
    Write-Error "No llama/ggml DLLs staged in $binariesDir. Run stage-llm-dlls.ps1 first."
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
