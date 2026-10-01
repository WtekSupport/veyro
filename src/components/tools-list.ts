import {
  getVocalSeparatorCapability,
  openAudioSrtToolWindowAndWaitReady,
  openDictationTranscriptsToolWindowAndWaitReady,
  openVocalSeparatorToolWindowAndWaitReady,
  openVoiceFilesToolWindowAndWaitReady,
} from "../api";
import { escapeHtml } from "../lib/html";
import { iconTools } from "./icons";
import { t } from "../i18n";

function renderToolsCards(
  voiceFilesOpening: boolean,
  dictationBuffersOpening: boolean,
  audioSrtOpening: boolean,
  vocalSepOpening: boolean,
  showVocalSep: boolean,
): string {
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
      <li>
        <button
          type="button"
          class="tools-card${dictationBuffersOpening ? " tools-card--loading" : ""}"
          data-open-dictation-transcripts-tool
          ${dictationBuffersOpening ? "disabled" : ""}
          aria-busy="${dictationBuffersOpening ? "true" : "false"}"
        >
          ${
            dictationBuffersOpening
              ? `<span class="tools-card-loading-badge">${escapeHtml(t("tools.dictationTranscripts.windowLoading"))}</span>`
              : ""
          }
          <span class="tools-card-title">${escapeHtml(t("tools.dictationTranscripts.title"))}</span>
          <span class="tools-card-desc">${escapeHtml(t("tools.dictationTranscripts.description"))}</span>
        </button>
      </li>
      <li>
        <button
          type="button"
          class="tools-card${audioSrtOpening ? " tools-card--loading" : ""}"
          data-open-audio-srt-tool
          ${audioSrtOpening ? "disabled" : ""}
          aria-busy="${audioSrtOpening ? "true" : "false"}"
        >
          ${
            audioSrtOpening
              ? `<span class="tools-card-loading-badge">${escapeHtml(t("tools.audioSrt.windowLoading"))}</span>`
              : ""
          }
          <span class="tools-card-title">${escapeHtml(t("tools.audioSrt.title"))}</span>
          <span class="tools-card-desc">${escapeHtml(t("tools.audioSrt.description"))}</span>
        </button>
      </li>
      ${
        showVocalSep
          ? `
      <li>
        <button
          type="button"
          class="tools-card${vocalSepOpening ? " tools-card--loading" : ""}"
          data-open-vocal-separator-tool
          ${vocalSepOpening ? "disabled" : ""}
          aria-busy="${vocalSepOpening ? "true" : "false"}"
        >
          ${
            vocalSepOpening
              ? `<span class="tools-card-loading-badge">${escapeHtml(t("tools.vocalSeparator.windowLoading"))}</span>`
              : ""
          }
          <span class="tools-card-title">${escapeHtml(t("tools.vocalSeparator.title"))}</span>
          <span class="tools-card-desc">${escapeHtml(t("tools.vocalSeparator.description"))}</span>
        </button>
      </li>
      `
          : ""
      }
    </ul>
  `;
}

/** Embedded tools panel — same chrome as status quick settings (НАСТРОЙКИ). */
export function renderToolsPanelEmbedded(
  voiceFilesOpening = false,
  dictationBuffersOpening = false,
  audioSrtOpening = false,
  vocalSepOpening = false,
  showVocalSep = false,
): string {
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
        ${renderToolsCards(voiceFilesOpening, dictationBuffersOpening, audioSrtOpening, vocalSepOpening, showVocalSep)}
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
export function renderToolsList(
  voiceFilesOpening = false,
  dictationBuffersOpening = false,
  audioSrtOpening = false,
  vocalSepOpening = false,
  showVocalSep = false,
): string {
  return renderToolsPanelEmbedded(
    voiceFilesOpening,
    dictationBuffersOpening,
    audioSrtOpening,
    vocalSepOpening,
    showVocalSep,
  );
}

function refreshToolsCards(host: HTMLElement): void {
  const flags = readOpeningFlags(host);
  const showVocalSep = host.dataset.vocalSepAvailable === "true";
  host.innerHTML = renderToolsCards(
    flags.voice,
    flags.dictation,
    flags.srt,
    flags.vocalSep,
    showVocalSep,
  );
  const panel = host.closest("[data-tools-panel-root]") ?? host;
  bindVoiceFilesOpen(panel);
  bindDictationTranscriptsOpen(panel);
  bindAudioSrtOpen(panel);
  bindVocalSepOpen(panel);
}

function readOpeningFlags(host: HTMLElement): {
  voice: boolean;
  dictation: boolean;
  srt: boolean;
  vocalSep: boolean;
} {
  return {
    voice: host.dataset.voiceFilesOpening === "true",
    dictation: host.dataset.dictationTranscriptsOpening === "true",
    srt: host.dataset.audioSrtOpening === "true",
    vocalSep: host.dataset.vocalSepOpening === "true",
  };
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
  refreshToolsCards(host);
  try {
    await openVoiceFilesToolWindowAndWaitReady();
  } catch (error) {
    console.error("open voice files tool", error);
  } finally {
    host.dataset.voiceFilesOpening = "false";
    refreshToolsCards(host);
  }
}

function bindDictationTranscriptsOpen(root: ParentNode): void {
  root
    .querySelector<HTMLButtonElement>("[data-open-dictation-transcripts-tool]")
    ?.addEventListener("click", () => {
      void openDictationTranscriptsFromCard(root);
    });
}

async function openDictationTranscriptsFromCard(root: ParentNode): Promise<void> {
  const panel = toolsPanelFromRoot(root);
  const host = panel.querySelector<HTMLElement>("[data-tools-cards-host]");
  if (!host) {
    return;
  }
  if (host.dataset.dictationTranscriptsOpening === "true") {
    return;
  }
  host.dataset.dictationTranscriptsOpening = "true";
  refreshToolsCards(host);
  try {
    await openDictationTranscriptsToolWindowAndWaitReady();
  } catch (error) {
    console.error("open dictation transcripts tool", error);
  } finally {
    host.dataset.dictationTranscriptsOpening = "false";
    refreshToolsCards(host);
  }
}

function bindAudioSrtOpen(root: ParentNode): void {
  root
    .querySelector<HTMLButtonElement>("[data-open-audio-srt-tool]")
    ?.addEventListener("click", () => {
      void openAudioSrtFromCard(root);
    });
}

async function openAudioSrtFromCard(root: ParentNode): Promise<void> {
  const panel = toolsPanelFromRoot(root);
  const host = panel.querySelector<HTMLElement>("[data-tools-cards-host]");
  if (!host) {
    return;
  }
  if (host.dataset.audioSrtOpening === "true") {
    return;
  }
  host.dataset.audioSrtOpening = "true";
  refreshToolsCards(host);
  try {
    await openAudioSrtToolWindowAndWaitReady();
  } catch (error) {
    console.error("open audio srt tool", error);
  } finally {
    host.dataset.audioSrtOpening = "false";
    refreshToolsCards(host);
  }
}

function bindVocalSepOpen(root: ParentNode): void {
  root
    .querySelector<HTMLButtonElement>("[data-open-vocal-separator-tool]")
    ?.addEventListener("click", () => {
      void openVocalSepFromCard(root);
    });
}

async function openVocalSepFromCard(root: ParentNode): Promise<void> {
  const panel = toolsPanelFromRoot(root);
  const host = panel.querySelector<HTMLElement>("[data-tools-cards-host]");
  if (!host) {
    return;
  }
  if (host.dataset.vocalSepOpening === "true") {
    return;
  }
  host.dataset.vocalSepOpening = "true";
  refreshToolsCards(host);
  try {
    await openVocalSeparatorToolWindowAndWaitReady();
  } catch (error) {
    console.error("open vocal separator tool", error);
  } finally {
    host.dataset.vocalSepOpening = "false";
    refreshToolsCards(host);
  }
}

export function bindToolsList(root: ParentNode): void {
  const host = root.querySelector<HTMLElement>("[data-tools-cards-host]");
  if (host && host.dataset.vocalSepCapabilityLoaded !== "true") {
    host.dataset.vocalSepCapabilityLoaded = "true";
    void getVocalSeparatorCapability()
      .then(({ available }) => {
        host.dataset.vocalSepAvailable = available ? "true" : "false";
        refreshToolsCards(host);
      })
      .catch(() => {
        host.dataset.vocalSepAvailable = "false";
        refreshToolsCards(host);
      });
  }
  bindVoiceFilesOpen(root);
  bindDictationTranscriptsOpen(root);
  bindAudioSrtOpen(root);
  bindVocalSepOpen(root);
}
