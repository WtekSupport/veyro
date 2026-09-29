# Настройки: авторасшифровка голосовых (`voice_watch`)

Блок в зашифрованном `config.json` (значения по умолчанию):

| Ключ | По умолчанию | Смысл |
|------|--------------|--------|
| `enabled` | `false` | Включён ли watcher папок |
| `folders` | `[]` | Абсолютные пути для наблюдения |
| `recursive` | `false` | Подпапки (глубина ≤ 2) |
| `extensions` | `ogg,oga,opus` | Допустимые расширения |
| `stable_ms` | `1500` | Стабильность размера/mtime |
| `max_size_mb` | `50` | Пропуск более крупных файлов |
| `max_duration_min` | `30` | Пропуск более длинных (только авто) |
| `name_filter` | `""` | Опциональный glob / `/regex/` |
| `only_local_providers` | `true` | Пауза при облачном STT/LLM |
| `text_mode_override` | `inherit` | `inherit` / `original` / `basic` / `skill:…` |
| `notify` | `result` | `off` / `result` / `result_and_copy` |
| `delete_source_after` | `false` | Удалять исходник после успеха |
| `move_source_after` | `false` | Перемещать в `Veyro/processed` |
| `history_retention_days` | `30` | Срок хранения истории |

Рядом с профилем: `voice-history.json`, `voice-watch-registry.json`.

UI: Инструменты → Обработка голосовых. Трей: чекбокс + «Последние расшифровки».

См. [voice-watch-research.md](voice-watch-research.md).
