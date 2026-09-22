# Building Veyro (English)

Detailed Russian guide: [BUILD.md](BUILD.md).

## Requirements

- Node.js 18+ (20+ recommended), npm, Rust stable
- Platform tools: see [Tauri prerequisites](https://tauri.app/start/prerequisites/)
- **Windows (primary):** WebView2, VS C++ build tools, CMake 3.20+ (scripts can bootstrap under `.tools/`), optional Vulkan SDK for GPU
- **Linux:** `./scripts/install-linux-dependencies.sh`
- **macOS:** Xcode CLT, CMake; use `npm run tauri:dev:mac` / `tauri:build:mac`

## Quick commands

| Goal | Command |
|------|---------|
| Dev (Windows) | `npm ci` then `npm run tauri:dev` |
| Release installer (Windows) | `npm run tauri:build` |
| Frontend only | `npm run build` |
| CI-like Rust check | `cd src-tauri && cargo test --no-default-features --lib && cargo clippy --no-default-features --lib -- -D warnings` |

Default Windows builds enable **local Whisper + sherpa-onnx STT (Parakeet / Qwen3-ASR) + local LLM** and pick the best GPU backend when SDKs are present.

Local STT uses **model family + quantization** in Voice settings (Whisper Q4/Q5/Q8 from HF `ggerganov/whisper.cpp`; Sherpa INT8 from [k2-fsa asr-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models); Parakeet FP16/FP32 from pinned Hugging Face archives). Sherpa bundles live under `{models}/sherpa/{family}/{int8|fp16|fp32}/`. Runtime EPs: **CoreML** (Apple Silicon), **DirectML** (Windows), **CUDA**, else CPU.

## Cloud-only / minimal build (matches GitHub CI)

```powershell
$env:VEYRO_DISABLE_LOCAL_WHISPER = "1"
$env:VEYRO_DISABLE_LOCAL_LLM = "1"
npm run tauri:dev
```

Or in `src-tauri`: `cargo test --no-default-features --lib`.

## Windows output paths

Release artifacts default to `C:\veyro-target` (override with `VEYRO_CARGO_TARGET_DIR`). Installers are copied under `release/<semver>/` (gitignored). See [RELEASE.md](RELEASE.md).

## Opt-out environment variables

| Variable | Effect |
|----------|--------|
| `VEYRO_DISABLE_GPU=1` | CPU-only Whisper/LLM |
| `VEYRO_DISABLE_LOCAL_LLM=1` | No on-device LLM |
| `VEYRO_DISABLE_LOCAL_WHISPER=1` | OpenAI STT only |
| `VEYRO_DISABLE_SHERPA_STT=1` | Omit sherpa-onnx (Parakeet / Qwen3); Whisper-only local STT |
| `VEYRO_DISABLE_VAD_SILERO=1` | Omit Silero VAD (WebRTC-only speech detection) |
| `VEYRO_CARGO_TARGET_DIR` | Custom Cargo target directory |

Full table: [BUILD.md](BUILD.md#отключение-функционала-opt-out).

## Contributing builds

Full signed release builds and updater keys are **maintainer-only**. See [CONTRIBUTING.md](../CONTRIBUTING.md) and [UPDATER.md](UPDATER.md).
