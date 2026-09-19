# Changelog

All notable user-facing changes are documented here and on [GitHub Releases](https://github.com/WtekSupport/veyro/releases).

Format: entries under `## [semver]` with date (UTC) when a release is published. Unreleased work lives under `## Unreleased`.

## Policy

- **Releases:** maintainers cut tagged releases (`v1.7.x`) and upload installers per [docs/RELEASE.md](docs/RELEASE.md).
- **Notes:** copy the relevant section into the GitHub Release description when publishing.
- **Contributors:** add a line under `## Unreleased` in PRs when user-visible behavior changes.

## Unreleased

## [1.8.7] — 2026-09-19

- **Local STT (sherpa-onnx):** Parakeet TDT 0.6B v3 and Qwen3-ASR 0.6B / 1.7B alongside Whisper; on-demand model download, unified `local_stt_model` setting (legacy `local_whisper_model` alias), provider factory and live preview for sherpa engines on Windows (DirectML/CUDA when enabled), macOS (CoreML), and CPU fallback.
- **Expert settings UI:** Reliable tab switching (LOG / Voice / Text & system); dynamic lists (microphones, transcription languages, STT models, diagnostics) load with retry instead of staying empty; fewer full-form redraws on status ticks so language and model changes stick.
- **Settings reliability:** Failed settings load on startup no longer writes default config over your saved file; bootstrap loads critical UI data in stages when the app controller is busy.
- **STT model picker:** Compact three-line option labels—model name and download size, CPU/GPU requirements, and features (languages, speed, accuracy, live preview).
- **Windows build & dev:** Serial MSBuild wrapper for llama.cpp Vulkan (`vulkan-shaders-gen` race mitigation), dev prebuild retry + `finish-llama-cpp-build`, documented `local-sherpa-stt` in BUILD guides and CI.

## [1.7.87] — 2026-09-18

- **PTT + AI post-processing (two phases):** While you hold PTT, text is cleaned with **Basic** (fast cleanup) and injected incrementally; after release, one session rewrite replaces that block with **Optimization** / custom skill output—no duplicate paragraphs from per-chunk AI during the hold.
- **Parallel replace UX:** Deleting the phase-1 block (visible backspace) runs in parallel with the LLM rewrite; the tray icon blinks purple during replacement, then optimized text is inserted.
- **Optimization output layout:** AI optimization now breaks long prose into readable paragraphs; dictionary corrections no longer strip paragraph breaks; Windows keyboard injection sends Enter for line/paragraph breaks so Word and similar editors show real paragraphs.
- **Punctuation & glue fixes:** Spaces after sentence punctuation when STT/AI chunks are merged (backend + status-bar preview on the frontend); PTT session buffer and dedupe passes normalize `word.Next` glitches.
- **Whisper turbo / Basic cleanup:** Deterministic cleanup for repeats, guillemets, fillers, orphan dot artifacts, and related STT noise in Basic mode.
- **PTT / VAD stability:** Fewer duplicate injections mid-hold (segment-on-silence, preview keep-alive, finish races); live preview during PTT+AI defer uses the same Basic cleanup as phase 1.
- **Tray & i18n:** Tooltip for AI text replacement; minor diagnostics/settings copy updates.

## [1.7.79] — 2026-09-17

- **Punctuation from pauses (local Whisper):** Commas and periods are inferred from timing gaps between decoded segments; on by default in Standard mode for local “Original” / light-cleanup paths.
- **Standard ↔ Expert text modes:** In Standard UI, **Original** runs the same light cleanup as Expert **Basic cleanup** (label unchanged). Switching UI modes maps **Original** ↔ **Basic** so the processing level is preserved.
- **Russian numbers as words:** Context-aware case inflection after converting digits to words (e.g. after “до”, “к”, “с”).
- **Settings:** Hide the OpenAI **Connections** block when transcription and text rewrite are fully on-device.
- **Automatic updates:** Signed Windows installer, portable zip, and `latest.json` manifest for the in-app updater (see [docs/UPDATER.md](docs/UPDATER.md)).
- **macOS:** Low-level hotkey and hardware-ID stability fixes for builds on Apple Silicon.

## [1.7.74] — 2026-09-17

- Desktop voice dictation app (Tauri): local Whisper, optional local LLM, OpenAI modes, updater via GitHub Releases.
