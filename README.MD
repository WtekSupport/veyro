# Veyro

[![CI](https://github.com/WtekSupport/veyro/actions/workflows/ci.yml/badge.svg)](https://github.com/WtekSupport/veyro/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/WtekSupport/veyro)](LICENSE)
[![Releases](https://img.shields.io/github/v/release/WtekSupport/veyro?label=release)](https://github.com/WtekSupport/veyro/releases)

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

Two options:

- **Local Whisper** — runs on your machine, works offline. Models are downloaded from the app (~140 MB up to ~3 GB). GPU acceleration speeds up recognition when available.
- **OpenAI Cloud** — transcription via OpenAI’s API. Requires an API key and internet.

Recognition language: automatic, Russian, or English.

---

## Text processing

After transcription, text can be refined further:

| Mode | What it does |
|------|----------------|
| **Original** | Raw Whisper output |
| **Basic cleanup** | Trims filler, fixes spacing and casing |
| **AI optimization** | AI removes disfluencies and ASR mistakes while keeping meaning |
| **Custom skill…** | Your own processing pipeline from a `.md` skill file (formal tone, brevity, etc.) |

For AI modes you choose a provider:

- **OpenAI** — via API (key required)
- **Local LLM** — on-device model (downloaded from the app)

### Extra text options

- **Spoken punctuation** — say “period”, “comma”, etc. to insert punctuation
- **Numbers as words** — e.g. `123` → “one hundred twenty-three” (locale-dependent)
- **Enter emulation** — phrases like “new line” send Enter

---

## Text injection

Transcribed text is delivered to the focused window by:

- **Clipboard** (paste)
- **Simulated keystrokes**

On macOS, keyboard injection requires **Accessibility** permission.

---

## Settings

- **Microphone** — device selection and level meter
- **UI language** — Russian or English
- **Launch at login**
- **Notifications** for app events
- **Advanced audio** — preprocessing before recognition, voice-activity (VAD) tuning

The **Status** tab shows an activity log and diagnostics (mic, API, models, queue).

---

## Privacy

- OpenAI API keys are stored in the **OS secure store** (Windows Credential Manager, macOS Keychain, Linux Secret Service), not in plain text.
- With **local Whisper** and **local LLM**, speech and text stay on your computer.
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
