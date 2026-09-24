# Changelog

All notable user-facing changes are documented here and on [GitHub Releases](https://github.com/WtekSupport/veyro/releases).

Format: entries under `## [semver]` with date (UTC) when a release is published. Unreleased work lives under `## Unreleased`.

## Policy

- **Releases:** maintainers cut tagged releases (`v1.7.x`) and upload installers per [docs/RELEASE.md](docs/RELEASE.md).
- **Notes:** copy the relevant section into the GitHub Release description when publishing.
- **Contributors:** add a line under `## Unreleased` in PRs when user-visible behavior changes.

## Unreleased

## [1.8.78] — 2026-09-24

### Added

- **Standard mode:** Silero VAD/TE download actions in any data-processing and text mode («Improve recognition» / «Improve punctuation»), two-column layout.
- **Standard mode:** Custom skill as a third text option synced with Expert; catalog «+» badge on the skill card.

### Fixed

- **Standard mode:** Persist `custom_skill` text mode (homemaker normalize no longer resets it to optimization).
- **Standard mode:** Vertical scroll for settings; skill card shows title only (no filename).

## [1.8.74] — 2026-09-24

### Added

- **Skills:** Block reinstall when the same `.md` is already in the skills folder; compact delete in text settings with in-app confirm dialog.
- **Skill import (veyro://):** Session-based deeplink flow so repeat installs open the window reliably; stable install filenames (catalog slug, not temp names).

### Fixed

- **Windows installer:** Bundle libtorch/MKL (`c10.dll`, `torch_cpu.dll`, …) next to `veyro.exe` again so the app starts after NSIS install.
- **Skill import UI:** Confirm install returns success/failure; re-enable buttons when confirmation is not pending.
- **Settings:** Remove open-skills-folder icon (import, delete, catalog remain).

## [1.8.65] — 2026-09-23

### Added

- **Skill import (veyro://):** Install text skills from aiStructEdit catalog links (`veyro://translate-en.md`, `veyro://import?path=…`, remote URL query). Permission preview, progress UI, copy into skills folder, auto-select skill in text settings.
- **Skill import window:** Dedicated `skill-import` webview with EN/RU strings; deeplink forwarded from single-instance and startup argv.

### Fixed

- **Skill deeplinks:** Catalog slug resolution before local path checks; CSP allows `aistructedit.com`; capabilities include `skill-import` webview (fixes spinner / invoke errors).
- **Windows REC overlay:** Frameless transparent surface — strip caption/sysmenu, disable DWM NC rendering and Win11 backdrop, re-apply after monitor move and show (no black plate or close button).

## [1.8.59] — 2026-09-23

### Fixed

- **Windows / Silero TE:** Ship full Intel MKL 2021.4 runtime (`mkl_def`, `mkl_avx2`, `mkl_avx512`, VML, etc.) with the app — fixes process exit with *Intel MKL FATAL ERROR* after Whisper STT when dispatch DLLs were missing next to the executable.
- **Windows / Silero TE:** `build.rs` and libtorch staging copy native DLLs beside `veyro.exe` and into `deps/` during dev builds; NSIS bundles all `mkl_*` libraries.
- **Sherpa Qwen3 (Windows):** CPU execution provider instead of DirectML; Sherpa runs on a dedicated thread with a global inference lock; native panics are caught and the recognizer can reload.
- **Silero TE:** Tokenizer `IntList` / 1D outputs normalized to `[1, seq]` before padding (fixes post-STT “index out of bounds” panic).
- **Text pipeline:** Post-STT cleanup runs on a blocking thread with panic isolation.
- **Local STT:** Sherpa/Whisper decode and post-STT work on blocking threads; skip background STT prewarm while a segment is queued.
- **Windows 10:** REC overlay no longer applies Win11-only DWM backdrop; overlay shows on the UI thread after the WebView loads.

### Added

- **Focus defer buffer:** With focus control off (default), dictation continues when the injection field loses focus; text buffers until focus returns.

## [1.8.56] — 2026-09-23

- **Windows / Silero TE:** Bundle full Intel MKL 2021.4 redist (`mkl_def`, `mkl_avx2`, VML, etc.) via `ensure-intel-mkl-redist.ps1` during libtorch staging — fixes fatal MKL errors after Whisper when dispatch DLLs were missing next to the exe.
- **Windows / Silero TE:** Set `MKL_DEBUG_CPU_TYPE` before libtorch loads as an extra safeguard on CPUs without AVX512 dispatch DLLs.

## [1.8.54] — 2026-09-23

- **Sherpa Qwen3 (Windows):** Use CPU execution provider instead of DirectML (avoids hard crash during decode); run sherpa on a dedicated thread with a global inference lock; catch native panics and reload the recognizer.
- **Text pipeline:** Catch panics during post-STT cleanup; harden Silero TE unicode token parsing.

## [1.8.52] — 2026-09-23

- **Silero TE:** Normalize tokenizer `IntList` / 1D token tensors to `[1, seq]` before padding (fixes “index out of bounds” panic during text processing after STT).

## [1.8.50] — 2026-09-22

- **Sherpa local STT crash:** Run sherpa-onnx decode/prewarm/unload on the blocking thread pool (same as Whisper). Skip background STT prewarm while a segment is queued; run post-STT cleanup (incl. Silero TE) on blocking threads. Fixes process exit when the tray turns red at transcription start on Windows.

## [1.8.49] — 2026-09-22

- **Windows 10 REC overlay crash:** Do not apply Win11-only DWM backdrop on Windows 10; show the recording overlay on the UI thread after the WebView loads (fixes exit when the red REC indicator appears).

## [1.8.48] — 2026-09-22

- **Focus defer buffer:** With **Focus control** off (new default), dictation keeps running when the injection field loses focus; text is buffered in memory and inserted when focus returns (already inserted text is kept).
- **PTT across windows:** Push-to-talk works while focus is in another app; injection target is preserved in defer mode; PTT state is reconciled after missed key-up (e.g. Alt+Tab).
- **Focus control default:** **Abort on focus loss** / **Контроль фокуса** is **off** by default — use the toggle to abort and clear queues on focus loss (previous default behavior).
- **REC overlay:** Smaller overlay window; DWM backdrop and frame tweaks to remove the visible square plate behind the REC indicator.
- **Settings:** Quick settings show the resolved **data storage** path; WebRTC VAD choice saves correctly in config (`webrtc` JSON key).
- **Text pipeline:** Silero TE tokenizer accepts TorchScript `IntList`; deferred flush uses a valid controller transition; minor Basic cleanup and normalization tweaks.

## [1.8.45] — 2026-09-22

- **Stability (weak PCs):** Narrower PTT release locking, inline PTT gate dispatch (no per-signal thread spawn), segment queue cap with activity logging, activity log disk cap, shared LLM engine handle; PTT hold always ends capture on key-up even while a prior segment is still transcribing.
- **REC overlay:** Fix stuck white overlay window on slow machines (hide HWND before clearing UI; show after page load; re-apply DWM transparency; overlay no longer reacts to global “listening stopped” during transcription).
- **Focus control (Windows):** Optional **Контроль фокуса** / **Focus control** — aborts dictation, clears pending recognition, and drops disk queue buffers when the injection target loses focus (already inserted text is kept).
- **Removed live preview:** Streaming partial transcription under the status bar and live dictation “…” indicator in the target field are gone (less CPU/GPU load; REC overlay remains).
- **Basic cleanup:** Fewer spurious mid-sentence capitals in **Basic** / fast PTT phase (abbreviations and proper-noun heuristics).
- **Settings UI:** Data storage row is a single **Choose folder** button; shorter focus-control label.
- **Release tooling:** GitHub publish script and Silero TE asset packaging updates.

## [1.8.39] — 2026-09-21

- **Local STT catalog:** Pick **family** and **quantization** (Whisper Q4–Q8, sherpa INT8/FP16/FP32) with a spec card—disk size, RAM/VRAM, speed & accuracy tiers, language summary—driven by `describe_local_stt_variant`.
- **Lossless segment queue:** VAD/PTT segments are queued instead of dropped when the pipeline is busy; optional **weak PC** profile spills overflow to disk under `{data_storage}/segment-queue/` with startup replay and cancel cleanup.
- **Inference memory:** Switching STT provider/model, cloud transcription model, LLM, or Silero TE purges loaded weights (transcriber replace + unload, Silero TE engine, idle/manual unload); activity `memory.inference_purged`.
- **Silero TE assets:** Weights are no longer bundled in the installer; the app downloads them on demand (GitHub release `silero-te-assets-v1`; publish via `scripts/package-silero-te-release.ps1`).
- **Status & settings UI:** Quick settings popover on Status (expert weak-PC options); STT picker component refactor; homemaker weak-PC switch only; overlay/status event wiring cleanup.
- **Silero VAD:** Dedicated on-disk model store path for Silero VAD weights (aligned with local model layout).

## [1.8.18] — 2026-09-20

- **Windows (Silero TE):** Bundle libtorch runtime DLLs (`c10.dll`, `torch_cpu.dll`, MKL/OpenMP, etc.) next to the app and in NSIS/portable builds via `stage-libtorch-dlls.ps1` — fixes startup error when Silero TE is enabled.

## [1.8.17] — 2026-09-20

- **Silero TE punctuation:** Optional on-device text enhancement (Silero TE via libtorch) replaces pause-based comma/period inference; bundled JIT assets, idle unload, setting **Silero TE** (migrates from legacy pause punctuation). Export script: `scripts/extract-silero-te.py`.
- **Expert Status tab:** Renamed from Log; live CPU/RAM for the app process tree (sysinfo), full-height mic level graph with VAD threshold overlay, capability chips instead of the old icon strip; activity log in a collapsible section.
- **Voice activity (VAD):** Silero VAD (wavekat-vad) as the default engine with WebRTC pre-filter on quiet frames; Expert toggle for WebRTC-only. Voice sensitivity maps to Silero probability threshold; VAD monitor always visible on Capture (no extra hint lines).
- **Settings UI (Capture / Transform):** Tab labels **Capture** and **Transform** (RU: Захват / Преобразование); recognition and activation sections regrouped; activation grid layout; removed separate **Game mode** / **Block key** controls—PTT hotkey blocking and overlay injection behavior are applied automatically on Windows.
- **Model pickers:** Local STT and local LLM dropdowns use three-line labels—name (download size), **RAM · CPU · GPU** requirements, and feature summary (aligned for both recognition and rewrite models).
- **STT cleanup:** Filters common Whisper subtitle hallucinations (e.g. “thanks for watching”, “DimaTorzok”-style tails) in normalization.
- **Overlay / PTT:** Hotkey registration prewarms overlay path; injection target restored after REC overlay show; fewer elevation/toast edge cases in push-to-talk mode.
- **Build & dev (Windows):** Empty Cargo default features so `tauri dev` matches script `--no-default-features` (fixes mixed-feature **LNK1120** link failures); optional `CARGO_INCREMENTAL=0` on dev; shared `Get-CargoFeatureArgs` for local feature sets; BUILD.md troubleshooting for linker errors.
- **Logging:** Default `tracing` filter quiets noisy `ort` / `ort_sys` crates unless overridden.

## [1.8.8] — 2026-09-19

- **Local STT (sherpa-onnx):** Parakeet TDT 0.6B v3 and Qwen3-ASR 0.6B / 1.7B alongside Whisper; on-demand model download, unified `local_stt_model` setting (legacy `local_whisper_model` alias), provider factory and live preview for sherpa engines on Windows (DirectML/CUDA when enabled), macOS (CoreML), and CPU fallback.
- **Expert settings UI:** Reliable tab switching (LOG / Voice / Text & system); dynamic lists (microphones, transcription languages, STT models, diagnostics) load with retry instead of staying empty; fewer full-form redraws on status ticks so language and model changes stick.
- **Settings reliability:** Failed settings load on startup no longer writes default config over your saved file; bootstrap loads critical UI data in stages when the app controller is busy.
- **STT model picker:** Compact three-line option labels—model name and download size, CPU/GPU requirements, and features (languages, speed, accuracy, live preview).
- **Windows build & dev:** Serial MSBuild wrapper for llama.cpp Vulkan (`vulkan-shaders-gen` race mitigation), dev prebuild retry + `finish-llama-cpp-build`, documented `local-sherpa-stt` in BUILD guides and CI.
- **Windows installer:** Bundle sherpa-onnx and ONNX Runtime DLLs next to the app (fixes missing `sherpa-onnx-c-api.dll` on install; portable zip includes the same runtime libraries). Release **1.8.7** was removed from GitHub—use **1.8.8** or newer.

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
