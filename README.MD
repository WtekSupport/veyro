<div align="center">

<img src="src/assets/logo.png" alt="Veyro logo" width="128" height="128" />

# Veyro

[![CI](https://github.com/WtekSupport/veyro/actions/workflows/ci.yml/badge.svg)](https://github.com/WtekSupport/veyro/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/WtekSupport/veyro)](LICENSE)
[![Releases](https://img.shields.io/github/v/release/WtekSupport/veyro?label=release)](https://github.com/WtekSupport/veyro/releases)

</div>

**Website:** [veyrotext.vercel.app](https://veyrotext.vercel.app)

**Veyro** is a desktop app for voice dictation. You speak — it transcribes your speech and inserts the result into the active window: a document, chat, browser, or code editor.

The app runs in the background and is opened from the **system tray** (Windows/Linux) or the **menu bar** (macOS).

---

## Quick start

1. Launch Veyro and turn it on in settings.
2. Focus the window where you want text.
3. Start dictating — the transcribed text is injected into the input field automatically.

### Recording modes

| Mode | Behavior |
|------|----------|
| **Push-to-talk (PTT)** | Hold a hotkey while speaking; release to transcribe and inject. Can be switched to tap-to-start / tap-to-stop. |
| **Continuous** | Speak naturally; after a pause, speech is transcribed and inserted automatically. |

The hotkey is configurable (default: `Shift+F1`). On macOS, **Caps Lock as the dictation key** is supported.

PTT plays short start/stop cues.

---

## Speech recognition

Two providers:

| Provider | Notes |
|----------|--------|
| **On device** | Works offline. Models are downloaded from the app. GPU acceleration (Vulkan, DirectML, CUDA, CoreML) speeds up inference when available. |
| **OpenAI Cloud** | Transcription via OpenAI’s API. Requires an API key and internet. |

**Local model families** (Voice settings):

- **Whisper** (whisper.cpp) — Base through Large v3 / Turbo; broad language support; Q4/Q5/Q8 quantizations.
- **Sherpa-onnx** — **Parakeet TDT 0.6B v3** and **Qwen3-ASR** bundles (INT8 / FP16 / FP32); English-focused or multilingual depending on model.

Recognition language: automatic, Russian, English, or another fixed language from the list.

**Voice activity (VAD):** Silero VAD by default (with a quiet-frame WebRTC pre-filter); optional WebRTC-only mode. Threshold can be auto or manual, with calibration from the mic.

A **transcription dictionary** (`dictionary.toml`) helps with names, jargon, and custom vocabulary.

---

## Text processing

After transcription, text can be refined further:

| Mode | What it does |
|------|----------------|
| **Original** | Raw STT output |
| **Basic cleanup** | Trims filler, fixes spacing and casing |
| **AI optimization** | AI removes disfluencies and ASR mistakes while keeping meaning |
| **Custom skill…** | Your own processing pipeline from a `.md` skill file (formal tone, brevity, etc.) |

For AI modes you choose a provider:

- **OpenAI** — via API (key required)
- **Local LLM** — on-device model (downloaded from the app)

Optional **Silero TE** adds on-device punctuation and text enhancement after STT (downloaded when enabled).

Skills can be imported from disk, the in-app catalog, or a `veyro://` deeplink; they live in your user **skills** folder.

### Extra text options

- **Spoken punctuation** — say “period”, “comma”, etc. to insert punctuation
- **Numbers as words** — e.g. `123` → “one hundred twenty-three” (locale-dependent)
- **Enter emulation** — phrases like “new line” send Enter

---

## Tools

Open **Tools** from the main settings window (or the status bar) for utilities that reuse the same speech engine and text pipeline as live dictation.

### Voice file processing

Transcribe audio files to text — queue multiple files, read the result, and copy to the clipboard. Processing uses your current STT provider and text mode (including **AI optimization** and **custom skills** when configured).

- **Input:** drag-and-drop or file picker — common formats such as WAV, MP3, OGG/Opus, FLAC, M4A, AAC, WMA.
- **Progress:** decoding and recognition show stage labels and a percentage for long files.
- **Safety:** file transcription is paused while PTT dictation is active; one file runs at a time.

### Audio to Subtitles (SRT)

Transcribe audio with segment timestamps and export `.srt` subtitles.

- Preview timeline with cue editing.
- Advanced layout: smart split, line limits, reading-tail options.
- Same STT provider and language settings as live dictation; if **Auto** language returns empty speech, the app can prompt for a fixed language and retry.

### Split vocals / instrumental

Offline stem separation for music tracks — downloadable ONNX models, progress UI, and preview/download for **vocals** and **instrumental** stems.

- Models include **Google ReFormer** (quality / speed) and **Hybrid Transformer Demucs**.
- Shown in Tools when the build includes local separation support.

---

## Text injection

Transcribed text is delivered to the focused window by:

- **Clipboard** (paste)
- **Simulated keystrokes**

On macOS, keyboard injection requires **Accessibility** permission.

---

## Settings

Two UI modes:

| Mode | Audience |
|------|----------|
| **Standard** | Simplified capture and text options |
| **Expert** | Full Capture / Transform / Status controls |

Also available:

- **Microphone** — device selection and level meter
- **UI language** — Russian or English
- **Launch at login**
- **Notifications** for app events
- **Advanced audio** — preprocessing before recognition, VAD engine and threshold
- **Weak PC** — optional segment queue spill to disk when the pipeline is busy
- **Tools** — voice files, SRT export, and stem separation

The **Status** tab shows live diagnostics (CPU/RAM, capability chips), an **input level** graph with VAD threshold, and a collapsible activity log.

---

## Privacy

- OpenAI API keys are stored in the **OS secure store** (Windows Credential Manager, macOS Keychain, Linux Secret Service), not in plain text.
- With **local speech models** (Whisper, Parakeet TDT, Qwen3-ASR via sherpa-onnx), **local LLM**, and **Silero TE**, speech and text stay on your computer.
- Stem separation also runs **on device** (ONNX); audio is not uploaded for that tool.
- Settings live in your user profile (`AppData\Veyro` on Windows, equivalent paths on macOS and Linux).

---

## Platforms

| OS | Install formats |
|----|-----------------|
| **Windows** | NSIS installer, portable `.exe` |
| **macOS** | `.app`, `.dmg` (macOS 11+) |
| **Linux** | `.deb`, AppImage |

Download installers from **[GitHub Releases](https://github.com/WtekSupport/veyro/releases)**.

Installed builds can check for signed updates on startup. See [docs/UPDATER.md](docs/UPDATER.md).

---

## Build from source

- English: [docs/BUILD.en.md](docs/BUILD.en.md)
- Русский: [docs/BUILD.md](docs/BUILD.md)

---

## Contributing

We welcome pull requests. Read [CONTRIBUTING.md](CONTRIBUTING.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Security reports: [`.github/SECURITY.md`](.github/SECURITY.md).

---

## License

[MIT](LICENSE) · © 2026 MKdee Tech
