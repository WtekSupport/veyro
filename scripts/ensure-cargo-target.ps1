# Default Windows build output directory (no spaces — required for llama.cpp Vulkan).

$DefaultCargoTarget = "C:\veyro-target"

if ($env:VEYRO_CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = $env:VEYRO_CARGO_TARGET_DIR
} else {
    $env:CARGO_TARGET_DIR = $DefaultCargoTarget
}

if (-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR)) {
    try {
        $null = New-Item -ItemType Directory -Path $env:CARGO_TARGET_DIR -Force
    } catch {
        Write-Error "Cannot create $($env:CARGO_TARGET_DIR). Run dev from an elevated shell (npm run tauri:dev and accept UAC)."
    }
}

Write-Host "CARGO_TARGET_DIR=$($env:CARGO_TARGET_DIR)"
