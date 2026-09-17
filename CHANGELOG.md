# Changelog

All notable user-facing changes are documented here and on [GitHub Releases](https://github.com/WtekSupport/veyro/releases).

Format: entries under `## [semver]` with date (UTC) when a release is published. Unreleased work lives under `## Unreleased`.

## Policy

- **Releases:** maintainers cut tagged releases (`v1.7.x`) and upload installers per [docs/RELEASE.md](docs/RELEASE.md).
- **Notes:** copy the relevant section into the GitHub Release description when publishing.
- **Contributors:** add a line under `## Unreleased` in PRs when user-visible behavior changes.

## Unreleased

- Open-source community scaffolding (CONTRIBUTING, CI, Dependabot, docs).

## [1.7.77] — 2026-09-17

### Standard mode (text)

- **«Оригинал»** now uses the same light cleanup as Expert **«Базовая очистка»** (capitalization, spacing, spoken punctuation, and related steps); the label in Standard stays «Оригинал».
- Switching **Standard ↔ Expert** maps **Оригинал ↔ Базовая очистка** so the same processing level is kept; legacy `basic` in Standard settings is normalized to **Оригинал**.

### Punctuation and numbers

- **Pause punctuation** (local Whisper): inserts `.` / `,` from segment gaps; enabled by default for local Standard **Оригинал** / Expert **Базовая очистка**.
- **Numbers as words (Russian):** inflects numerals by context (e.g. «до двадцати») when **Числа прописью** is on.

### UI and settings

- Hide the **OpenAI connections** block in Standard when transcription and rewrite are fully local.
- Removed the long hint under **Pause punctuation**; option remains in settings.

### Platform

- macOS: low-level hotkey and HWID stability fixes for builds and licensing.

## [1.7.74] — 2026-09-17

- Desktop voice dictation app (Tauri): local Whisper, optional local LLM, OpenAI modes, updater via GitHub Releases.
