import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateProgress = {
  phase: "idle" | "checking" | "downloading" | "installing" | "error";
  percent?: number;
  message?: string;
};

export type UpdateCheckResult =
  | { status: "unavailable" }
  | { status: "current" }
  | { status: "available"; update: Update }
  | { status: "error"; message: string };

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
    return { status: "error", message };
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
