/**
 * Regenerates docs/legal/third-party-licenses.json from Cargo (release feature set)
 * and npm lockfile. Run from repo root after dependency changes:
 *   node scripts/generate-third-party-licenses.mjs
 */
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = path.join(root, "src-tauri");
const outPath = path.join(root, "docs", "legal", "third-party-licenses.json");

/** Bundled native / runtime components not fully described by a single crate row. */
const NATIVE_RUNTIME = [
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
    name: "llama.cpp",
    copyright: "Copyright © Georgi Gerganov and llama.cpp contributors",
    license: "MIT",
    url: "https://github.com/ggml-org/llama.cpp",
  },
  {
    name: "ONNX Runtime",
    copyright: "Copyright © Microsoft Corporation and ONNX Runtime contributors",
    license: "MIT",
    url: "https://github.com/microsoft/onnxruntime",
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
];

function resolveReleaseFeatures() {
  return execSync(
    `powershell -NoProfile -ExecutionPolicy Bypass -File "${path.join(root, "scripts", "print-release-cargo-features.ps1")}"`,
    { cwd: root, encoding: "utf8" },
  )
    .trim()
    .split(/\r?\n/)
    .pop()
    .trim();
}

function loadCargoMetadata(features) {
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
  return JSON.parse(json.trim());
}

function collectResolvedPackages(metadata) {
  const rootPkg = metadata.packages.find((p) => p.name === "veyro");
  if (!rootPkg || !metadata.resolve) {
    throw new Error("cargo metadata missing veyro resolve graph");
  }
  const ids = new Set();
  const visit = (id) => {
    if (ids.has(id)) return;
    ids.add(id);
    const node = metadata.resolve.nodes.find((n) => n.id === id);
    if (!node) return;
    for (const dep of node.deps ?? []) {
      visit(dep.pkg);
    }
  };
  visit(rootPkg.id);
  return [...ids]
    .map((id) => metadata.packages.find((p) => p.id === id))
    .filter((p) => p && p.name !== "veyro");
}

function formatCopyright(pkg) {
  const authors = pkg.authors?.filter(Boolean) ?? [];
  if (authors.length > 0) {
    return `Copyright © ${authors.join(", ")}`;
  }
  return "See project repository";
}

function formatUrl(pkg) {
  if (pkg.repository) return pkg.repository;
  if (pkg.homepage) return pkg.homepage;
  return `https://crates.io/crates/${pkg.name}`;
}

function crateEntry(pkg) {
  return {
    name: `${pkg.name} ${pkg.version}`,
    copyright: formatCopyright(pkg),
    license: pkg.license?.trim() || "See crate metadata",
    url: formatUrl(pkg),
  };
}

function loadNpmPackages() {
  const lockPath = path.join(root, "package-lock.json");
  let lock;
  try {
    lock = JSON.parse(readFileSync(lockPath, "utf8"));
  } catch {
    return [];
  }
  const packages = lock.packages ?? {};
  const entries = [];
  for (const [pkgPath, info] of Object.entries(packages)) {
    if (!pkgPath || pkgPath === "") continue;
    if (!info.name || !info.version) continue;
    const name = info.name;
    const key = `${name}@${info.version}`;
    if (entries.some((e) => e._key === key)) continue;
    const license = info.license ?? "See package metadata";
    entries.push({
      _key: key,
      name: `${name} ${info.version}`,
      copyright: "See package repository",
      license: typeof license === "string" ? license : JSON.stringify(license),
      url: info.resolved?.startsWith("http")
        ? info.resolved.replace(/#.*$/, "")
        : `https://www.npmjs.com/package/${name}`,
    });
  }
  return entries.map(({ _key, ...rest }) => rest);
}

function sortByName(list) {
  return [...list].sort((a, b) =>
    a.name.localeCompare(b.name, "en", { sensitivity: "base" }),
  );
}

function main() {
  const features = resolveReleaseFeatures();
  const metadata = loadCargoMetadata(features);
  const crates = collectResolvedPackages(metadata).map(crateEntry);
  const npm = loadNpmPackages();

  const seen = new Set();
  const merged = [];
  for (const entry of [...NATIVE_RUNTIME, ...crates, ...npm]) {
    const dedupeKey = entry.name.toLowerCase();
    if (seen.has(dedupeKey)) continue;
    seen.add(dedupeKey);
    merged.push(entry);
  }

  const sorted = sortByName(merged);
  mkdirSync(path.dirname(outPath), { recursive: true });
  writeFileSync(outPath, `${JSON.stringify(sorted, null, 2)}\n`, "utf8");
  console.log(`Wrote ${sorted.length} entries to ${outPath}`);
}

main();
