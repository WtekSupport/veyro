import { execSync } from "node:child_process";

const PORT = 1420;

function freePortWindows(port) {
  try {
    const output = execSync(`netstat -ano | findstr :${port} | findstr LISTENING`, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });

    const pids = new Set();
    for (const line of output.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed) {
        continue;
      }
      const pid = trimmed.split(/\s+/).at(-1);
      if (pid && pid !== "0") {
        pids.add(pid);
      }
    }

    for (const pid of pids) {
      try {
        execSync(`taskkill /PID ${pid} /F`, { stdio: "ignore" });
        console.log(`[dev] freed port ${port} (stopped PID ${pid})`);
      } catch {
        // Process may have already exited.
      }
    }
  } catch {
    // Nothing listening on the port.
  }
}

function freePortUnix(port) {
  try {
    execSync(`lsof -ti:${port} | xargs -r kill -9`, {
      stdio: "ignore",
      shell: true,
    });
    console.log(`[dev] freed port ${port}`);
  } catch {
    // Nothing listening on the port.
  }
}

function killStaleDevAppWindows() {
  try {
    execSync("taskkill /IM veyro.exe /F", { stdio: "ignore" });
    console.log("[dev] stopped previous veyro.exe");
  } catch {
    // No dev app running.
  }
}

if (process.platform === "win32") {
  killStaleDevAppWindows();
  freePortWindows(PORT);
} else {
  try {
    execSync("pkill -f 'target/debug/veyro' || true", {
      stdio: "ignore",
      shell: true,
    });
  } catch {
    // No dev app running.
  }
  freePortUnix(PORT);
}
