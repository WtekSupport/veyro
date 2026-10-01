#!/usr/bin/env python3
"""Veyro Canary-Qwen 2.5B sidecar worker (NeMo SALM + optional Optimum Quanto int8).

Protocol: one JSON object per stdin line.
  {"cmd":"transcribe","wav_path":"...","sample_rate":16000}
  {"cmd":"quit"}
Responses: one JSON object per stdout line.
  {"ok":true,"text":"..."}
  {"ok":false,"error":"..."}
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path


def log(msg: str) -> None:
    print(msg, file=sys.stderr, flush=True)


def load_model(bundle: Path):
    """Load nvidia/canary-qwen-2.5b via NeMo SALM; quantize LLM with Optimum Quanto when available."""
    try:
        from nemo.collections.speechlm2.models import SALM  # type: ignore
    except Exception as exc:  # pragma: no cover
        raise RuntimeError(
            "NeMo SALM is required for Canary-Qwen. "
            "Install: pip install nemo_toolkit[asr] optimum-quanto"
        ) from exc

    cache_dir = bundle / "hf_cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    os.environ.setdefault("HF_HOME", str(cache_dir))
    os.environ.setdefault("HUGGINGFACE_HUB_CACHE", str(cache_dir / "hub"))

    model_id = "nvidia/canary-qwen-2.5b"
    log(f"loading {model_id}…")
    model = SALM.from_pretrained(model_id)

    # Optional INT8 on the LLM component (speech encoder stays full precision).
    try:
        from optimum.quanto import quantize, qint8, freeze  # type: ignore

        llm = getattr(model, "llm", None) or getattr(model, "language_model", None)
        if llm is not None:
            log("applying Optimum Quanto qint8 to LLM component")
            quantize(llm, weights=qint8)
            freeze(llm)
        else:
            log("LLM attribute not found; skipping quanto")
    except Exception as exc:
        log(f"quanto unavailable or failed ({exc}); running BF16/FP16")

    model.eval()
    return model


def transcribe(model, wav_path: str) -> str:
    # NeMo SALM generate API: prompt for ASR mode.
    prompt = "Transcribe the following: "
    try:
        # Prefer generate with audio path if supported by this NeMo version.
        out = model.generate(
            prompts=[prompt],
            audios=[wav_path],
        )
    except TypeError:
        # Fallback: some builds take a manifest-like dict.
        out = model.generate([{"prompt": prompt, "audio": wav_path}])

    if isinstance(out, (list, tuple)) and out:
        item = out[0]
        if isinstance(item, str):
            return item.strip()
        if isinstance(item, dict):
            return str(item.get("text") or item.get("pred_text") or item).strip()
        return str(item).strip()
    return str(out).strip()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", required=True)
    args = parser.parse_args()
    bundle = Path(args.bundle)
    bundle.mkdir(parents=True, exist_ok=True)

    model = None
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as exc:
            print(json.dumps({"ok": False, "error": f"bad json: {exc}"}), flush=True)
            continue

        cmd = req.get("cmd")
        if cmd == "quit":
            break
        if cmd != "transcribe":
            print(json.dumps({"ok": False, "error": f"unknown cmd: {cmd}"}), flush=True)
            continue

        wav_path = req.get("wav_path")
        if not wav_path or not Path(wav_path).is_file():
            print(json.dumps({"ok": False, "error": "wav_path missing"}), flush=True)
            continue

        try:
            if model is None:
                model = load_model(bundle)
            text = transcribe(model, wav_path)
            print(json.dumps({"ok": True, "text": text}), flush=True)
        except Exception as exc:
            print(json.dumps({"ok": False, "error": str(exc)}), flush=True)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
