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

Default Windows builds enable **local Whisper + local LLM** and pick the best GPU backend when SDKs are present.

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
| `VEYRO_CARGO_TARGET_DIR` | Custom Cargo target directory |

Full table: [BUILD.md](BUILD.md#отключение-функционала-opt-out).

## Contributing builds

Full signed release builds and updater keys are **maintainer-only**. See [CONTRIBUTING.md](../CONTRIBUTING.md) and [UPDATER.md](UPDATER.md).
