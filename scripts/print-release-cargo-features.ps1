# Writes release Cargo feature set (one line, comma-separated) to stdout only.
$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot
. (Join-Path $PSScriptRoot "ensure-rust-path.ps1")
. (Join-Path $PSScriptRoot "resolve-local-features.ps1")
$features = Resolve-LocalFeatures -RepoRoot $repoRoot
if (-not $features) {
    # Cloud-only fallback; still list runtime Rust deps from Cargo.toml.
    $features = "local-whisper-vulkan,local-llm-vulkan,local-sherpa-stt,vad-silero,silero-te"
}
Write-Output $features
