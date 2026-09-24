import { openVoiceFilesToolWindowAndWaitReady } from "../api";
import { iconTools } from "./icons";
import { t } from "../i18n";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function renderToolsCards(voiceFilesOpening: boolean): string {
  return `
    <ul class="tools-list" role="list">
      <li>
        <button
          type="button"
          class="tools-card${voiceFilesOpening ? " tools-card--loading" : ""}"
          data-open-voice-files-tool
          ${voiceFilesOpening ? "disabled" : ""}
          aria-busy="${voiceFilesOpening ? "true" : "false"}"
        >
          ${
            voiceFilesOpening
              ? `<span class="tools-card-loading-badge">${escapeHtml(t("tools.voiceFiles.windowLoading"))}</span>`
              : ""
          }
          <span class="tools-card-title">${escapeHtml(t("tools.voiceFiles.title"))}</span>
          <span class="tools-card-desc">${escapeHtml(t("tools.voiceFiles.description"))}</span>
        </button>
      </li>
    </ul>
  `;
}

/** Embedded tools panel — same chrome as status quick settings (НАСТРОЙКИ). */
export function renderToolsPanelEmbedded(voiceFilesOpening = false): string {
  return `
    <div class="status-quick-settings-panel tools-in-main-panel" data-tools-panel-root>
      <header class="status-quick-settings-header">
        <h2 class="status-quick-settings-title" id="tools-panel-title">
          ${escapeHtml(t("tools.title"))}
        </h2>
        <button
          type="button"
          class="status-quick-settings-close icon-btn"
          data-close-tools-panel
          aria-label="${escapeHtml(t("common.close"))}"
          title="${escapeHtml(t("common.close"))}"
        >×</button>
      </header>
      <div class="status-quick-settings-body tools-in-main-body" data-tools-cards-host>
        ${renderToolsCards(voiceFilesOpening)}
      </div>
    </div>
  `;
}

export function renderToolsBadgeButton(active: boolean): string {
  const label = t("tools.open");
  return `
    <button
      type="button"
      class="tools-badge-btn icon-btn${active ? " tools-badge-btn--active" : ""}"
      data-open-tools-panel
      title="${escapeHtml(label)}"
      aria-label="${escapeHtml(label)}"
      aria-pressed="${active ? "true" : "false"}"
    >${iconTools()}</button>
  `;
}

/** @deprecated Standalone tools window — use embedded panel in main UI. */
export function renderToolsList(voiceFilesOpening = false): string {
  return renderToolsPanelEmbedded(voiceFilesOpening);
}

function refreshToolsCards(host: HTMLElement, opening: boolean): void {
  host.innerHTML = renderToolsCards(opening);
  bindVoiceFilesOpen(host.closest("[data-tools-panel-root]") ?? host);
}

function bindVoiceFilesOpen(root: ParentNode): void {
  root
    .querySelector<HTMLButtonElement>("[data-open-voice-files-tool]")
    ?.addEventListener("click", () => {
      void openVoiceFilesFromCard(root);
    });
}

function toolsPanelFromRoot(root: ParentNode): HTMLElement {
  if (root instanceof Element) {
    return root.closest<HTMLElement>("[data-tools-panel-root]") ?? (root as HTMLElement);
  }
  return document.querySelector<HTMLElement>("[data-tools-panel-root]") ?? document.body;
}

async function openVoiceFilesFromCard(root: ParentNode): Promise<void> {
  const panel = toolsPanelFromRoot(root);
  const host = panel.querySelector<HTMLElement>("[data-tools-cards-host]");
  if (!host) {
    return;
  }
  if (host.dataset.voiceFilesOpening === "true") {
    return;
  }
  host.dataset.voiceFilesOpening = "true";
  refreshToolsCards(host, true);
  try {
    await openVoiceFilesToolWindowAndWaitReady();
  } catch (error) {
    console.error("open voice files tool", error);
  } finally {
    host.dataset.voiceFilesOpening = "false";
    refreshToolsCards(host, false);
  }
}

export function bindToolsList(root: ParentNode): void {
  bindVoiceFilesOpen(root);
}
