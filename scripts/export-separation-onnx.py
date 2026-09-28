#!/usr/bin/env python3
"""Minimal helper to export a vocal-separation checkpoint to ONNX (offline maintainer tool)."""

from __future__ import annotations

import argparse


def main() -> None:
    parser = argparse.ArgumentParser(description="Export vocal separation model to ONNX")
    parser.add_argument("--checkpoint", required=True, help="Path to PyTorch checkpoint")
    parser.add_argument("--output", required=True, help="Output .onnx path")
    parser.add_argument(
        "--sample-frames",
        type=int,
        default=485_100,
        help="Fixed time dimension for export (44.1 kHz stereo)",
    )
    args = parser.parse_args()

    raise SystemExit(
        "Implement export for your checkpoint using the MSS training repo.\n"
        f"  checkpoint: {args.checkpoint}\n"
        f"  output: {args.output}\n"
        f"  sample_frames: {args.sample_frames}\n"
        "See scripts/export-separation-onnx.md"
    )


if __name__ == "__main__":
    main()
