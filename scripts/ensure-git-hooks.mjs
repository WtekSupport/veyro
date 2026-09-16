import { execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const hooksPath = path.join(root, "githooks");

try {
  execSync("git rev-parse --git-dir", { cwd: root, stdio: "ignore" });
} catch {
  process.exit(0);
}

const relativeHooks = path.relative(root, hooksPath).replaceAll("\\", "/");
execSync(`git config core.hooksPath "${relativeHooks}"`, { cwd: root, stdio: "inherit" });
console.log(`Git hooks: core.hooksPath=${relativeHooks}`);
