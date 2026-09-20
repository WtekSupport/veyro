#!/usr/bin/env python3
"""Export Silero TE JIT assets for Veyro (from v2_4lang_q.pt)."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

import torch
from torch import package


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--package",
        default="https://models.silero.ai/te_models/v2_4lang_q.pt",
        help="Silero TE torch package (.pt) path or URL",
    )
    parser.add_argument(
        "--out",
        default="src-tauri/resources/silero-te",
        help="Output directory for model.pt, tokenizer.pt, meta.json",
    )
    args = parser.parse_args()

    package_path = args.package
    if package_path.startswith("http"):
        cache = Path(args.out).parent / "v2_4lang_q.pt"
        cache.parent.mkdir(parents=True, exist_ok=True)
        if not cache.is_file():
            torch.hub.download_url_to_file(package_path, str(cache), progress=True)
        package_path = str(cache)

    imp = package.PackageImporter(package_path)
    model = imp.load_pickle("te_model", "model")

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    torch.jit.save(model.model, out / "model.pt")
    torch.jit.save(model.tokenizer, out / "tokenizer.pt")

    meta = {
        "pad": model.pad,
        "pad_token": str(model.tokenizer.pad_token),
        "uni_vocab": [str(item) for item in list(model.tokenizer.uni_vocab)],
    }
    (out / "meta.json").write_text(
        json.dumps(meta, ensure_ascii=False),
        encoding="utf-8",
    )
    print(f"Wrote Silero TE assets to {out.resolve()}")


if __name__ == "__main__":
    main()
