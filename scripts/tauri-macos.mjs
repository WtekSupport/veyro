// macOS build/dev: full local stack (Whisper Metal + local LLM) by default.
// Opt-out: VEYRO_DISABLE_GPU, VEYRO_DISABLE_LOCAL_LLM, VEYRO_DISABLE_LOCAL_WHISPER.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const [command, ...extraArgs] = process.argv.slice(2);

if (command !== "dev" && command !== "build") {
  console.error("usage: node scripts/tauri-macos.mjs <dev|build> [tauri args]");
  process.exit(1);
}

if (process.platform !== "darwin") {
  console.error("[tauri-macos] macOS only; on Windows use npm run tauri:dev / tauri:build");
  process.exit(1);
}

const bumpMode = command === "build" ? "prod" : "dev";
const bump = spawnSync(process.execPath, [path.join(root, "scripts", "bump-version.mjs"), bumpMode], {
  stdio: "inherit",
  cwd: root,
});
if (bump.status !== 0) {
  process.exit(bump.status ?? 1);
}

/** @returns {string[]} */
function resolveFeatures() {
  const features = [];
  if (process.env.VEYRO_DISABLE_LOCAL_WHISPER !== "1") {
    if (process.env.VEYRO_DISABLE_GPU === "1") {
      features.push("local-whisper");
    } else {
      features.push(process.env.VEYRO_WHISPER_FEATURE || "local-whisper-metal");
    }
  }
  if (process.env.VEYRO_DISABLE_LOCAL_LLM !== "1") {
    features.push(process.env.VEYRO_LLM_FEATURE || "local-llm");
  }
  return features;
}

const features = resolveFeatures();
const args = [command];
if (features.length > 0) {
  args.push("--features", features.join(","));
}
if (process.arch === "arm64" && !extraArgs.includes("--target")) {
  args.push("--target", "aarch64-apple-darwin");
}
args.push(...extraArgs);

console.log(`[tauri-macos] tauri ${args.join(" ")}`);
const result = spawnSync("npm", ["run", "tauri", "--", ...args], { stdio: "inherit", cwd: root });
process.exit(result.status ?? 1);
