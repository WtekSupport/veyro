# Voice watch — codebase research (TZ §3)

Research for auto transcription of messenger voice files (`todo/veyro-auto-voice-transcription-tz.md`).

## Stack map

| Area | Finding |
|------|---------|
| Stack | Tauri 2 + Rust (`src-tauri`), Vite/TypeScript frontend |
| Voice file processing | [`src-tauri/src/tools/voice_file.rs`](../src-tauri/src/tools/voice_file.rs) — `transcribe_voice_file`; UI queue in [`src/components/tool-voice-files-ui.ts`](../src/components/tool-voice-files-ui.ts) |
| Queue today | Frontend-only; one job at a time via `runQueue`; backend serializes with `ToolsTranscriptionGuard` |
| PTT vs tools | Guard returns `tools.voiceFiles.pttBusy` at start (no wait/resume) — fixed for shared queue |
| History | Not persisted before this feature |
| Settings | Encrypted JSON `AppSettings` in `{config}/Veyro/config.json`; nested `voice_watch` block with `#[serde(default)]` |
| Tray | Flat menu in [`src-tauri/src/tray/menu.rs`](../src-tauri/src/tray/menu.rs); CheckMenuItem/Submenu added for voice watch |
| Notifications | [`src-tauri/src/notify.rs`](../src-tauri/src/notify.rs) — title/body; gated by `show_notifications` |
| i18n | TS `src/i18n/locales/{en,ru}.ts` + Rust `src-tauri/src/i18n/mod.rs` |
| Activity log | `AppContext::record_activity` → Status tab |
| File watcher | `notify` crate already used for skills ([`text/skill.rs`](../src-tauri/src/text/skill.rs)) |

## Messenger save paths (Stage 1 notes)

Verified from public docs / community reports (not hardcoding unverified client cache paths into presets):

| Client | Typical user-facing save path | Notes |
|--------|-------------------------------|-------|
| **Telegram Desktop** | `{Downloads}/Telegram Desktop` | Default download path; user may change under Settings → Advanced. Voice often saved as `.ogg` / `audio_*.ogg`. Linux may use `TelegramDesktop`. |
| **Element Desktop** | System Downloads (or save dialog) | Electron `will-download` / save dialog — no fixed subfolder. Cover with Downloads preset. |
| **WhatsApp Desktop** | Manual save → user-chosen / Downloads | Auto cache under AppData packages is opaque and out of v1 scope. |
| **Max** | Treat as Downloads unless verified otherwise | No reliable public default; custom folder supported. |

**Presets shipped:** system Downloads, Telegram Desktop (under Downloads), custom folder via dialog.

## Architecture choice

Shared Rust queue + history so watching works with the tool window closed. Manual enqueue uses the same API. See `src-tauri/src/tools/voice_watch/`.
