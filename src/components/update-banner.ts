import type { Update } from "@tauri-apps/plugin-updater";

import {
  checkForAppUpdate,
  downloadAndInstallUpdate,
  type UpdateProgress,
} from "../updater";
import { t } from "../i18n";

let mountedHost: HTMLElement | null = null;
let pendingUpdate: Update | null = null;
let busy = false;

function renderBanner(host: HTMLElement, progress: UpdateProgress | null): void {
  if (!pendingUpdate && !progress) {
    host.innerHTML = "";
    host.hidden = true;
    return;
  }

  host.hidden = false;
  const version = pendingUpdate?.version ?? "";
  const isWorking =
    progress?.phase === "checking" ||
    progress?.phase === "downloading" ||
    progress?.phase === "installing";

  let body = t("update.available", { version });
  if (progress?.phase === "checking") {
    body = t("update.checking");
  } else if (progress?.phase === "downloading") {
    body = t("update.downloading", {
      percent: String(progress.percent ?? 0),
    });
  } else if (progress?.phase === "installing") {
    body = t("update.installing");
  } else if (progress?.phase === "error") {
    body = progress.message ?? t("update.failed");
  }

  host.innerHTML = `
    <div class="update-banner" role="status">
      <div class="update-banner-text">${escapeHtml(body)}</div>
      <div class="update-banner-actions">
        ${
          isWorking
            ? ""
            : `<button type="button" class="btn btn-primary update-banner-install">${escapeHtml(t("update.install"))}</button>`
        }
        ${
          isWorking
            ? ""
            : `<button type="button" class="btn btn-secondary update-banner-dismiss">${escapeHtml(t("update.later"))}</button>`
        }
      </div>
    </div>
  `;

  const installBtn = host.querySelector<HTMLButtonElement>(".update-banner-install");
  installBtn?.addEventListener("click", () => {
    void runInstall(host);
  });

  const dismissBtn = host.querySelector<HTMLButtonElement>(".update-banner-dismiss");
  dismissBtn?.addEventListener("click", () => {
    pendingUpdate = null;
    renderBanner(host, null);
  });
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

async function runInstall(host: HTMLElement): Promise<void> {
  if (!pendingUpdate || busy) {
    return;
  }
  busy = true;
  const update = pendingUpdate;
  try {
    await downloadAndInstallUpdate(update, (progress) => {
      renderBanner(host, progress);
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    renderBanner(host, { phase: "error", message });
    busy = false;
  }
}

export async function runStartupUpdateCheck(
  host: HTMLElement,
  enabled: boolean,
): Promise<void> {
  mountedHost = host;
  if (!enabled) {
    host.hidden = true;
    return;
  }

  renderBanner(host, { phase: "checking" });
  const result = await checkForAppUpdate();

  if (result.status === "available") {
    pendingUpdate = result.update;
    renderBanner(host, null);
    return;
  }

  pendingUpdate = null;
  host.hidden = true;
  host.innerHTML = "";
}

export function mountUpdateBannerSlot(): HTMLElement {
  const host = document.querySelector<HTMLElement>("#update-banner-slot");
  if (!host) {
    throw new Error("update-banner-slot missing from index.html");
  }
  return host;
}

export function dismissUpdateBanner(): void {
  if (mountedHost) {
    pendingUpdate = null;
    mountedHost.hidden = true;
    mountedHost.innerHTML = "";
  }
}
