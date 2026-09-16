# Default Windows build output directory (no spaces — required for llama.cpp Vulkan).

$DefaultCargoTarget = "C:\veyro-target"

if ($env:VEYRO_CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = $env:VEYRO_CARGO_TARGET_DIR
} else {
    $env:CARGO_TARGET_DIR = $DefaultCargoTarget
}

Write-Host "CARGO_TARGET_DIR=$($env:CARGO_TARGET_DIR)"
