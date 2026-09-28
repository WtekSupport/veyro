# Offline ONNX export for vocal separation models

This repository ships **pre-exported ONNX weights** downloaded at runtime from upstream URLs
(see `src-tauri/src/separation/model_store.rs`). Use this guide when you need to refresh or
replace those weights.

## Mel-Band RoFormer (quality / fast)

1. Clone [Music-Source-Separation-Training](https://github.com/ZFTurbo/Music-Source-Separation-Training) or use an upstream Hugging Face checkpoint.
2. Export to ONNX with a fixed stereo input shape `[1, 2, N]` (float32), vocal-only head.
3. Validate with ONNX Runtime on a short WAV (44.1 kHz stereo).
4. Update `MODELS` URLs and `min_bytes` in `model_store.rs`.

## Demucs legacy (`htdemucs`)

1. Export `htdemucs` / `htdemucs_ft` to ONNX with stereo mix input.
2. Confirm output tensor layout matches `veyro-separation` stem extraction in `roformer.rs`.
3. Publish to a stable URL and update the `Legacy` entry in `model_store.rs`.

## Script

Run `scripts/export-separation-onnx.py --help` for a minimal PyTorch → ONNX stub (requires your
local checkpoint paths and Python env with `torch`, `onnx`, and the model code).
