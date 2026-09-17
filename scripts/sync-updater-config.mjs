import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const confPath = path.join(root, "src-tauri", "tauri.conf.json");
const pubPath = path.join(root, "src-tauri", "updater.pub");

const conf = JSON.parse(readFileSync(confPath, "utf8"));

let pubkey = "";
try {
  pubkey = readFileSync(pubPath, "utf8").trim();
} catch {
  console.warn(
    "Updater: src-tauri/updater.pub missing — run scripts/ensure-updater-keys.ps1",
  );
}

const enabled = pubkey.length > 0;

if (!conf.bundle) {
  conf.bundle = {};
}
conf.bundle.createUpdaterArtifacts = enabled;

if (enabled) {
  conf.plugins = conf.plugins ?? {};
  conf.plugins.updater = {
    pubkey,
    endpoints: [
      "https://github.com/WtekSupport/veyro/releases/latest/download/latest.json",
    ],
    windows: {
      installMode: "passive",
    },
  };
} else if (conf.plugins?.updater) {
  delete conf.plugins.updater;
}

writeFileSync(confPath, `${JSON.stringify(conf, null, 2)}\n`, "utf8");
console.log(`Updater config: ${enabled ? "enabled" : "disabled"}`);
