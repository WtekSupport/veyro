/**
 * Regenerates docs/legal/third-party-licenses.json with **key technologies**
 * only (STT/LLM/VAD/audio/UI runtimes), not the full Cargo/npm tree.
 *
 *   node scripts/generate-third-party-licenses.mjs
 *
 * Optional: enrich crate versions from `cargo metadata` when Rust is on PATH.
 */
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = path.join(root, "src-tauri");
const outPath = path.join(root, "docs", "legal", "third-party-licenses.json");

/**
 * Product-facing / notable third-party technologies shown in About.
 * `crate` — optional Cargo package name used to fill the version when metadata is available.
 */
const KEY_TECHNOLOGIES = [
  {
    name: "Tauri",
    crate: "tauri",
    copyright: "Copyright © Tauri Programme within The Commons Conservancy",
    license: "MIT OR Apache-2.0",
    url: "https://github.com/tauri-apps/tauri",
  },
  {
    name: "Microsoft Edge WebView2",
    copyright: "Copyright © Microsoft Corporation",
    license: "Microsoft Software License Terms (runtime component)",
    url: "https://developer.microsoft.com/microsoft-edge/webview2/",
  },
  {
    name: "whisper.cpp",
    copyright: "Copyright © Georgi Gerganov and whisper.cpp contributors",
    license: "MIT",
    url: "https://github.com/ggml-org/whisper.cpp",
  },
  {
    name: "whisper-rs",
    crate: "whisper-rs",
    copyright: "Copyright © whisper-rs contributors",
    license: "BSD-3-Clause",
    url: "https://codeberg.org/tazz4843/whisper-rs",
  },
  {
    name: "llama.cpp",
    copyright: "Copyright © Georgi Gerganov and llama.cpp contributors",
    license: "MIT",
    url: "https://github.com/ggml-org/llama.cpp",
  },
  {
    name: "llama-cpp-2",
    crate: "llama-cpp-2",
    copyright: "Copyright © utilityai / llama-cpp-rs contributors",
    license: "MIT OR Apache-2.0",
    url: "https://github.com/utilityai/llama-cpp-rs",
  },
  {
    name: "sherpa-onnx",
    crate: "sherpa-onnx",
    copyright: "Copyright © Next-gen Kaldi / sherpa-onnx contributors",
    license: "Apache-2.0",
    url: "https://github.com/k2-fsa/sherpa-onnx",
  },
  {
    name: "GigaAM",
    copyright: "Copyright © SberDevices / GigaAM contributors",
    license: "MIT",
    url: "https://github.com/salute-developers/GigaAM",
  },
  {
    name: "NVIDIA Canary-Qwen",
    copyright: "Copyright © NVIDIA Corporation",
    license: "CC-BY-4.0",
    url: "https://huggingface.co/nvidia/canary-qwen-2.5b",
  },
  {
    name: "IBM Granite Speech",
    copyright: "Copyright © IBM Corporation",
    license: "Apache-2.0",
    url: "https://huggingface.co/ibm-granite/granite-speech-3.3-8b",
  },
  {
    name: "CrispASR",
    copyright: "Copyright © CrispStrobe / CrispASR contributors",
    license: "See project repository",
    url: "https://github.com/CrispStrobe/CrispASR",
  },
  {
    name: "ONNX Runtime",
    copyright: "Copyright © Microsoft Corporation and ONNX Runtime contributors",
    license: "MIT",
    url: "https://github.com/microsoft/onnxruntime",
  },
  {
    name: "HT-Demucs (htdemucs / htdemucs_6s)",
    copyright: "Copyright © Meta Platforms, Inc. and Demucs contributors",
    license: "MIT",
    url: "https://github.com/facebookresearch/demucs",
  },
  {
    name: "StemSplitio HT-Demucs ONNX exports",
    copyright: "Copyright © StemSplit / demucs-onnx contributors",
    license: "See model cards (htdemucs-ft-vocals-onnx, htdemucs-6s-onnx)",
    url: "https://huggingface.co/StemSplitio",
  },
  {
    name: "Silero VAD",
    copyright: "Copyright © Silero Team",
    license: "MIT",
    url: "https://github.com/snakers4/silero-vad",
  },
  {
    name: "Silero TE (text enhancement)",
    copyright: "Copyright © Silero Team",
    license: "See Silero TE model / project terms",
    url: "https://github.com/snakers4/silero-models",
  },
  {
    name: "PyTorch / LibTorch",
    copyright: "Copyright © PyTorch contributors",
    license: "BSD-3-Clause",
    url: "https://github.com/pytorch/pytorch",
  },
  {
    name: "Intel oneMKL",
    copyright: "Copyright © Intel Corporation",
    license: "Intel Simplified Software License",
    url: "https://www.intel.com/content/www/us/en/developer/tools/oneapi/onemkl.html",
  },
  {
    name: "WebRTC VAD",
    crate: "webrtc-vad",
    copyright: "Copyright © webrtc-vad / WebRTC contributors",
    license: "BSD-3-Clause",
    url: "https://github.com/kaegi/webrtc-vad",
  },
  {
    name: "nnnoiseless (RNNoise)",
    crate: "nnnoiseless",
    copyright: "Copyright © nnnoiseless / RNNoise contributors",
    license: "BSD-3-Clause OR Apache-2.0 OR MIT",
    url: "https://github.com/jneem/nnnoiseless",
  },
  {
    name: "cpal",
    crate: "cpal",
    copyright: "Copyright © RustAudio / cpal contributors",
    license: "Apache-2.0 OR MIT",
    url: "https://github.com/rustaudio/cpal",
  },
  {
    name: "rodio",
    crate: "rodio",
    copyright: "Copyright © RustAudio / rodio contributors",
    license: "MIT OR Apache-2.0",
    url: "https://github.com/RustAudio/rodio",
  },
  {
    name: "Symphonia",
    crate: "symphonia",
    copyright: "Copyright © Philip Deljanov and Symphonia contributors",
    license: "MPL-2.0",
    url: "https://github.com/pdeljanov/Symphonia",
  },
  {
    name: "wasapi (Windows loopback)",
    crate: "wasapi",
    copyright: "Copyright © wasapi crate contributors",
    license: "MIT OR Apache-2.0",
    url: "https://crates.io/crates/wasapi",
  },
  {
    name: "enigo",
    crate: "enigo",
    copyright: "Copyright © enigo contributors",
    license: "MIT",
    url: "https://github.com/enigo-rs/enigo",
  },
  {
    name: "global-hotkey",
    crate: "global-hotkey",
    copyright: "Copyright © Tauri Programme within The Commons Conservancy",
    license: "MIT OR Apache-2.0",
    url: "https://github.com/tauri-apps/global-hotkey",
  },
  {
    name: "OpenAI API (optional cloud STT / rewrite)",
    copyright: "Copyright © OpenAI",
    license: "OpenAI API Terms of Use (service; not redistributed as code)",
    url: "https://openai.com/policies/terms-of-use",
  },
];

function tryLoadCrateVersions() {
  try {
    const features = execSync(
      `powershell -NoProfile -ExecutionPolicy Bypass -File "${path.join(root, "scripts", "print-release-cargo-features.ps1")}"`,
      { cwd: root, encoding: "utf8" },
    )
      .trim()
      .split(/\r?\n/)
      .pop()
      .trim();

    const ps = [
      `$ErrorActionPreference = 'Stop'`,
      `Set-Location '${tauriDir.replace(/'/g, "''")}'`,
      `. '${path.join(root, "scripts", "ensure-rust-path.ps1").replace(/'/g, "''")}'`,
      `cargo metadata --format-version=1 --features '${features.replace(/'/g, "''")}'`,
    ].join("; ");

    const json = execSync(`powershell -NoProfile -ExecutionPolicy Bypass -Command "${ps}"`, {
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });
    const metadata = JSON.parse(json.trim());
    const versions = new Map();
    for (const pkg of metadata.packages ?? []) {
      versions.set(pkg.name, pkg.version);
    }
    return versions;
  } catch (error) {
    console.warn(
      `cargo metadata unavailable (${error.message?.split("\n")[0] ?? error}); writing curated list without crate versions.`,
    );
    return new Map();
  }
}

function sortByName(list) {
  return [...list].sort((a, b) =>
    a.name.localeCompare(b.name, "en", { sensitivity: "base" }),
  );
}

function main() {
  const versions = tryLoadCrateVersions();
  const entries = KEY_TECHNOLOGIES.map((tech) => {
    const version = tech.crate ? versions.get(tech.crate) : null;
    return {
      name: version ? `${tech.name} ${version}` : tech.name,
      copyright: tech.copyright,
      license: tech.license,
      url: tech.url,
    };
  });

  const sorted = sortByName(entries);
  mkdirSync(path.dirname(outPath), { recursive: true });
  writeFileSync(outPath, `${JSON.stringify(sorted, null, 2)}\n`, "utf8");
  console.log(`Wrote ${sorted.length} key-technology entries to ${outPath}`);
}

main();
