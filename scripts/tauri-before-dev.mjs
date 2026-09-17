import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const node = process.execPath;

spawnSync(node, [path.join(root, "scripts", "free-dev-port.mjs")], {
  stdio: "inherit",
  cwd: root,
});

const viteEntry = path.join(root, "node_modules", "vite", "bin", "vite.js");
const child = spawn(node, [viteEntry], { stdio: "inherit", cwd: root });

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
