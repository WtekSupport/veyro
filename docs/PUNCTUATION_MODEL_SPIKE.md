# Phase 2 spike: compact punctuation restoration models

MVP in Veyro uses **Whisper segment gaps** ([`pause_punctuation.rs`](../src-tauri/src/text/pause_punctuation.rs)) — no extra model.

## Goal

Improve comma/period quality for Basic/Original without the LLM rewrite pipeline.

## Candidates (not integrated)

| Option | Notes |
|--------|--------|
| [silero-models punctuation](https://github.com/snakers4/silero-models) | RU/EN, small ONNX, commercial-friendly check per release |
| [deepmultilingualpunctuation](https://github.com/oliverguhr/deepmultilingualpunctuation) | Multilingual BERT-style, heavier (~500MB+) |
| [sberbank-ai ruPunct](https://huggingface.co) | RU-focused, verify license and ONNX export |

## Integration sketch

1. Optional setting `punctuation_model_enabled` (separate from pause heuristics).
2. Download to `%AppData%/Veyro/models/punct/` like Whisper.
3. Run after pause punctuation or replace it when model loaded.
4. Target latency: &lt;200 ms for a typical dictation block on CPU.

## Decision

Defer until MVP pause punctuation is validated on real RU dictation. Re-evaluate Silero or a distilled RU model first.
