#!/usr/bin/env node
/**
 * Forces serial cmake --build for llama.cpp (vulkan-shaders-gen MSBuild race).
 * Invoked from .tools/veyro-cmake-wrapper/cmake.bat — Node parses argv safely on Windows.
 */
import { spawnSync } from "node:child_process";

const realCmake = process.argv[2];
if (!realCmake) {
  console.error("cmake-build-serial.mjs: missing path to real cmake.exe");
  process.exit(1);
}

/** @type {string[]} */
let cmakeArgs = process.argv.slice(3);

const out = [];
let parallelSet = false;
for (let i = 0; i < cmakeArgs.length; i++) {
  const arg = cmakeArgs[i];
  if (arg === "--parallel" || arg === "-j") {
    parallelSet = true;
    if (i + 1 < cmakeArgs.length && !cmakeArgs[i + 1].startsWith("-")) {
      i++;
    }
    continue;
  }
  if (arg.startsWith("--parallel=")) {
    parallelSet = true;
    continue;
  }
  out.push(arg);
}

out.push("--parallel", "1");
void parallelSet;

let hasMsbuildSerial = out.some(
  (a) => /\/m:1\b/i.test(a) || /BuildInParallel=false/i.test(a),
);
if (!hasMsbuildSerial) {
  out.push("--", "/m:1", "/p:BuildInParallel=false");
}

const result = spawnSync(realCmake, out, { stdio: "inherit", shell: false });
process.exit(result.status ?? 1);
