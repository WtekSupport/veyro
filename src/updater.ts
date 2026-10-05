import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

import { t } from "./i18n";

export type UpdateProgress = {
  phase: "idle" | "checking" | "downloading" | "installing" | "error";
  percent?: number;
  /** Technical detail (logs / secondary line in UI). */
  message?: string;
  errorKind?: "check" | "install";
};

export type UpdateCheckResult =
  | { status: "unavailable" }
  | { status: "current" }
  | { status: "available"; update: Update }
  | { status: "error"; message: string; userMessage: string };

type UpdateErrorKind = "network" | "not_found" | "generic";

function classifyUpdateError(message: string): UpdateErrorKind {
  const lower = message.toLowerCase();
  if (
    /error sending request|failed to fetch|network|timed out|timeout|connect|connection|dns|offline|unreachable|tls|certificate|proxy/i.test(
      lower,
    )
  ) {
    return "network";
  }
  if (/404|not found|could not find/i.test(lower)) {
    return "not_found";
  }
  return "generic";
}

export function formatUpdateErrorMessage(
  raw: string,
  context: "check" | "install",
): { title: string; detail: string | null } {
  const message = raw.trim();
  const kind = classifyUpdateError(message);
  let title: string;
  if (context === "check") {
    if (kind === "network") {
      title = t("update.checkFailedNetwork");
    } else if (kind === "not_found") {
      title = t("update.checkFailedNotFound");
    } else {
      title = t("update.checkFailed");
    }
  } else if (kind === "network") {
    title = t("update.downloadFailedNetwork");
  } else {
    title = t("update.failed");
  }
  const detail = message.length > 0 && message !== title ? message : null;
  return { title, detail };
}

export function formatUpdateCheckFailure(raw: string): {
  userMessage: string;
  message: string;
} {
  const { title, detail } = formatUpdateErrorMessage(raw, "check");
  return { userMessage: title, message: detail ?? raw };
}

export async function checkForAppUpdate(): Promise<UpdateCheckResult> {
  try {
    const update = await check();
    if (!update) {
      return { status: "current" };
    }
    return { status: "available", update };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (/not configured|pubkey|updater/i.test(message)) {
      return { status: "unavailable" };
    }
    const formatted = formatUpdateCheckFailure(message);
    return {
      status: "error",
      message: formatted.message,
      userMessage: formatted.userMessage,
    };
  }
}

export async function downloadAndInstallUpdate(
  update: Update,
  onProgress?: (progress: UpdateProgress) => void,
): Promise<void> {
  onProgress?.({ phase: "downloading", percent: 0 });

  let downloaded = 0;
  let contentLength = 0;

  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        contentLength = event.data.contentLength ?? 0;
        onProgress?.({ phase: "downloading", percent: 0 });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        if (contentLength > 0) {
          onProgress?.({
            phase: "downloading",
            percent: Math.min(100, Math.round((downloaded / contentLength) * 100)),
          });
        }
        break;
      case "Finished":
        onProgress?.({ phase: "installing", percent: 100 });
        break;
      default:
        break;
    }
  });

  onProgress?.({ phase: "installing" });
  await relaunch();
}
