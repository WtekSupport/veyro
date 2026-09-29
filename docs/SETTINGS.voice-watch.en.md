//! Settings docs: voice watch (auto transcription of messenger voice files).

## `voice_watch` (AppSettings)

Encrypted JSON settings block (defaults):

| Key | Default | Meaning |
|-----|---------|---------|
| `enabled` | `false` | Folder watcher on/off |
| `folders` | `[]` | Absolute paths to watch |
| `recursive` | `false` | Include subfolders (max depth 2) |
| `extensions` | `ogg,oga,opus` | Allowed extensions |
| `stable_ms` | `1500` | Wait for size/mtime stability |
| `max_size_mb` | `50` | Skip larger files |
| `max_duration_min` | `30` | Skip longer audio (auto only) |
| `name_filter` | `""` | Optional glob / `/regex/` |
| `only_local_providers` | `true` | Pause watch when cloud STT/LLM is active |
| `text_mode_override` | `inherit` | `inherit` / `original` / `basic` / `skill:…` |
| `notify` | `result` | `off` / `result` / `result_and_copy` |
| `delete_source_after` | `false` | Delete source after success |
| `move_source_after` | `false` | Move to `Veyro/processed` |
| `history_retention_days` | `30` | History purge window |

Persisted beside settings: `{config}/Veyro/voice-history.json`, `voice-watch-registry.json`.

UI: Tools → Voice file processing. Tray: checkbox + Recent transcripts.

See [voice-watch-research.md](voice-watch-research.md).
