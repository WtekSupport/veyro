# Changelog

All notable user-facing changes are documented here and on [GitHub Releases](https://github.com/WtekSupport/veyro/releases).

Format: entries under `## [semver]` with date (UTC) when a release is published. Unreleased work lives under `## Unreleased`.

## Policy

- **Releases:** maintainers cut tagged releases (`v1.7.x`) and upload installers per [docs/RELEASE.md](docs/RELEASE.md).
- **Notes:** copy the relevant section into the GitHub Release description when publishing.
- **Contributors:** add a line under `## Unreleased` in PRs when user-visible behavior changes.

## Unreleased

## [1.7.79] — 2026-09-17

- **Punctuation from pauses (local Whisper):** Commas and periods are inferred from timing gaps between decoded segments; on by default in Standard mode for local “Original” / light-cleanup paths.
- **Standard ↔ Expert text modes:** In Standard UI, **Original** runs the same light cleanup as Expert **Basic cleanup** (label unchanged). Switching UI modes maps **Original** ↔ **Basic** so the processing level is preserved.
- **Russian numbers as words:** Context-aware case inflection after converting digits to words (e.g. after “до”, “к”, “с”).
- **Settings:** Hide the OpenAI **Connections** block when transcription and text rewrite are fully on-device.
- **Automatic updates:** Signed Windows installer, portable zip, and `latest.json` manifest for the in-app updater (see [docs/UPDATER.md](docs/UPDATER.md)).
- **macOS:** Low-level hotkey and hardware-ID stability fixes for builds on Apple Silicon.

## [1.7.74] — 2026-09-17

- Desktop voice dictation app (Tauri): local Whisper, optional local LLM, OpenAI modes, updater via GitHub Releases.
