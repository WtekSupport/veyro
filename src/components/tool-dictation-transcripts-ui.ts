import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  EVENTS,
  listDictationTranscripts,
  removeDictationTranscript,
  removeVoiceIndexEntry,
  type DictationTranscriptEntry,
  type VoiceFileProgressPhase,
} from "../api";
import { iconCopy } from "./icons";
import {
  loadToolBrowserSidebarCollapsed,
  renderToolBrowserSidebar,
  saveToolBrowserSidebarCollapsed,
  toolBrowserShellClass,
  type ToolBrowserSidebarLabels,
} from "../lib/tool-browser-sidebar";
import { escapeHtml } from "../lib/html";
import {
  fetchVoiceIndex,
  getVoiceIndexSelection,
  setVoiceIndexSelection,
  subscribeVoiceIndex,
  voiceIndexStatusLabel,
  type VoiceIndexEntry,
} from "../lib/tool-voice-index";
import { t } from "../i18n";

type SidebarPanel = "session" | "file";

function formatSessionLabel(entry: DictationTranscriptEntry): string {
  const d = new Date(entry.startedAtMs);
  if (Number.isNaN(d.getTime())) {
    return entry.id;
  }
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function previewLine(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  if (!flat) {
    return t("tools.dictationTranscripts.emptyPreview");
  }
  return flat.length > 48 ? `${flat.slice(0, 48)}…` : flat;
}

function statusLabel(status: DictationTranscriptEntry["status"]): string {
  return status === "active"
    ? t("tools.dictationTranscripts.status.active")
    : t("tools.dictationTranscripts.status.done");
}

function renderVoiceFileRows(
  entries: VoiceIndexEntry[],
  selectedId: string | null,
  progressByPath: Map<string, { phase: VoiceFileProgressPhase; percent: number | null }>,
): string {
  if (entries.length === 0) {
    return `<li class="voice-files-browser-empty muted">${escapeHtml(t("tools.voiceFiles.indexEmpty"))}</li>`;
  }
  return entries
    .map((entry) => {
      const selected = entry.id === selectedId;
      const prog = progressByPath.get(entry.path);
      const statusText = voiceIndexStatusLabel(
        entry.status,
        prog?.phase ?? null,
        prog?.percent ?? null,
      );
      const canRemove = entry.status !== "processing";
      return `
            <li class="voice-files-browser-row${selected ? " voice-files-browser-row--selected" : ""} voice-files-browser-row--${escapeHtml(entry.status)}"
                data-voice-file-id="${escapeHtml(entry.id)}">
              <button type="button" class="voice-files-browser-main" data-select-voice-file="${escapeHtml(entry.id)}">
                <span class="voice-files-browser-name">${escapeHtml(entry.fileName)}</span>
                <span class="voice-files-browser-status">${escapeHtml(statusText)}</span>
              </button>
              <button type="button" class="voice-files-browser-remove" data-remove-voice-file="${escapeHtml(entry.id)}"
                ${canRemove ? "" : "disabled"}
                aria-label="${escapeHtml(t("tools.voiceWatch.removeFromIndex"))}"
                title="${escapeHtml(t("tools.voiceWatch.removeFromIndex"))}">×</button>
            </li>`;
    })
    .join("");
}

const DICTATION_SIDEBAR_TOOL_ID = "dictation-transcripts";

function dictationSidebarLabels(): ToolBrowserSidebarLabels {
  return {
    ariaLabel: t("tools.dictationTranscripts.sidebarTitle"),
    title: t("tools.dictationTranscripts.sidebarTitle"),
    collapseLabel: t("tools.dictationTranscripts.sidebarCollapse"),
    expandLabel: t("tools.dictationTranscripts.sidebarExpand"),
  };
}

function renderSidebar(
  sessions: DictationTranscriptEntry[],
  selectedSessionId: string | null,
  voiceFiles: VoiceIndexEntry[],
  selectedVoiceFileId: string | null,
  activePanel: SidebarPanel,
  progressByPath: Map<string, { phase: VoiceFileProgressPhase; percent: number | null }>,
  collapsed: boolean,
): string {
  const sessionRows =
    sessions.length === 0
      ? `<li class="voice-files-browser-empty muted">${escapeHtml(t("tools.dictationTranscripts.emptyList"))}</li>`
      : sessions
          .map((entry) => {
            const selected =
              activePanel === "session" && entry.id === selectedSessionId;
            return `
            <li class="voice-files-browser-row${selected ? " voice-files-browser-row--selected" : ""} voice-files-browser-row--${escapeHtml(entry.status)}"
                data-session-id="${escapeHtml(entry.id)}">
              <button type="button" class="voice-files-browser-main" data-select-session="${escapeHtml(entry.id)}">
                <span class="voice-files-browser-name">${escapeHtml(formatSessionLabel(entry))}</span>
                <span class="voice-files-browser-status">${escapeHtml(statusLabel(entry.status))} · ${escapeHtml(previewLine(entry.text))}</span>
              </button>
              <button type="button" class="voice-files-browser-remove" data-remove-session="${escapeHtml(entry.id)}"
                aria-label="${escapeHtml(t("tools.dictationTranscripts.delete"))}"
                title="${escapeHtml(t("tools.dictationTranscripts.delete"))}">×</button>
            </li>`;
          })
          .join("");

  const fileRows = renderVoiceFileRows(
    voiceFiles,
    activePanel === "file" ? selectedVoiceFileId : null,
    progressByPath,
  );

  const body = `
      <div class="voice-files-sidebar-section">
        <p class="voice-files-sidebar-section-title">${escapeHtml(t("tools.dictationTranscripts.sidebarTitle"))}</p>
        <ul class="voice-files-browser" role="list">${sessionRows}</ul>
      </div>
      <div class="voice-files-sidebar-section">
        <p class="voice-files-sidebar-section-title">${escapeHtml(t("tools.voiceFiles.sidebarTitle"))}</p>
        <ul class="voice-files-browser" role="list">${fileRows}</ul>
      </div>`;

  return renderToolBrowserSidebar(dictationSidebarLabels(), collapsed, body);
}

function renderTool(args: {
  sessions: DictationTranscriptEntry[];
  selectedSessionId: string | null;
  voiceFiles: VoiceIndexEntry[];
  selectedVoiceFileId: string | null;
  activePanel: SidebarPanel;
  progressByPath: Map<string, { phase: VoiceFileProgressPhase; percent: number | null }>;
  copyHint: string | null;
  sidebarCollapsed: boolean;
}): string {
  const {
    sessions,
    selectedSessionId,
    voiceFiles,
    selectedVoiceFileId,
    activePanel,
    progressByPath,
    copyHint,
    sidebarCollapsed,
  } = args;

  const selectedSession =
    sessions.find((e) => e.id === selectedSessionId) ??
    sessions.find((e) => e.status === "active") ??
    sessions[0] ??
    null;
  const selectedFile =
    voiceFiles.find((e) => e.id === selectedVoiceFileId) ?? null;

  let displayText = "";
  let hintLine = "";
  if (activePanel === "file" && selectedFile) {
    displayText = selectedFile.text;
    hintLine = `${voiceIndexStatusLabel(selectedFile.status, null, null)} · ${selectedFile.fileName}`;
  } else if (selectedSession) {
    displayText = selectedSession.text;
    hintLine = `${statusLabel(selectedSession.status)} · ${formatSessionLabel(selectedSession)}`;
  }

  const canCopy = Boolean(displayText.trim());
  const copyTitle = copyHint ?? t("tools.dictationTranscripts.copy");

  return `
    <div class="${toolBrowserShellClass(sidebarCollapsed)}">
      ${renderSidebar(
        sessions,
        selectedSession?.id ?? null,
        voiceFiles,
        selectedVoiceFileId,
        activePanel,
        progressByPath,
        sidebarCollapsed,
      )}
      <div class="voice-files-main">
        <div class="voice-files-text-stack" aria-live="polite">
          <div class="voice-files-text-wrap">
            <button type="button" class="voice-files-copy-overlay icon-btn" data-copy-result
              ${canCopy ? "" : "disabled"}
              aria-label="${escapeHtml(copyTitle)}"
              title="${escapeHtml(copyTitle)}">${iconCopy()}</button>
            <textarea class="voice-files-textarea" readonly spellcheck="false"
              data-result-text
              aria-label="${escapeHtml(t("tools.dictationTranscripts.resultTitle"))}"
              placeholder="${escapeHtml(t("tools.dictationTranscripts.emptyResult"))}"
            >${escapeHtml(displayText)}</textarea>
          </div>
          ${hintLine ? `<p class="voice-files-stage-hint">${escapeHtml(hintLine)}</p>` : ""}
          ${copyHint ? `<p class="voice-files-copy-hint">${escapeHtml(copyHint)}</p>` : ""}
        </div>
      </div>
    </div>
  `;
}

export function createDictationTranscriptsController(root: HTMLElement): void {
  let sessions: DictationTranscriptEntry[] = [];
  let voiceFiles: VoiceIndexEntry[] = [];
  let selectedSessionId: string | null = null;
  let selectedVoiceFileId: string | null = null;
  let activePanel: SidebarPanel = "session";
  let copyHint: string | null = null;
  let sidebarCollapsed = loadToolBrowserSidebarCollapsed(DICTATION_SIDEBAR_TOOL_ID);
  let copyHintTimer: number | null = null;
  const progressByPath = new Map<
    string,
    { phase: VoiceFileProgressPhase; percent: number | null }
  >();
  const unlisteners: UnlistenFn[] = [];
  let indexUnsubscribe: (() => void) | null = null;

  const refreshVoiceFiles = async (): Promise<void> => {
    try {
      const snapshot = await fetchVoiceIndex();
      voiceFiles = snapshot.index;
      if (
        selectedVoiceFileId &&
        !voiceFiles.some((e) => e.id === selectedVoiceFileId)
      ) {
        selectedVoiceFileId = null;
        if (activePanel === "file") {
          activePanel = "session";
        }
      }
    } catch {
      voiceFiles = [];
    }
  };

  const paint = (): void => {
    root.innerHTML = renderTool({
      sessions,
      selectedSessionId,
      voiceFiles,
      selectedVoiceFileId,
      activePanel,
      progressByPath,
      copyHint,
      sidebarCollapsed,
    });
    bind();
  };

  const selectBestSession = (): void => {
    if (selectedSessionId && sessions.some((e) => e.id === selectedSessionId)) {
      return;
    }
    selectedSessionId =
      sessions.find((e) => e.status === "active")?.id ?? sessions[0]?.id ?? null;
  };

  const refreshSessions = async (): Promise<void> => {
    try {
      sessions = await listDictationTranscripts();
    } catch (error) {
      console.error("list dictation transcripts", error);
      sessions = [];
    }
    selectBestSession();
  };

  const refresh = async (): Promise<void> => {
    await Promise.all([refreshSessions(), refreshVoiceFiles()]);
    const persisted = getVoiceIndexSelection();
    if (persisted && voiceFiles.some((e) => e.id === persisted)) {
      selectedVoiceFileId = persisted;
      activePanel = "file";
    }
    paint();
  };

  const bind = (): void => {
    root.querySelector("[data-toggle-sidebar]")?.addEventListener("click", () => {
      sidebarCollapsed = !sidebarCollapsed;
      saveToolBrowserSidebarCollapsed(DICTATION_SIDEBAR_TOOL_ID, sidebarCollapsed);
      paint();
    });

    root.querySelectorAll<HTMLButtonElement>("[data-select-session]").forEach((btn) => {
      btn.addEventListener("click", () => {
        selectedSessionId = btn.dataset.selectSession ?? null;
        activePanel = "session";
        paint();
      });
    });

    root.querySelectorAll<HTMLButtonElement>("[data-select-voice-file]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.dataset.selectVoiceFile ?? null;
        selectedVoiceFileId = id;
        activePanel = "file";
        if (id) {
          setVoiceIndexSelection(id);
        }
        paint();
      });
    });

    root.querySelectorAll<HTMLButtonElement>("[data-remove-session]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.dataset.removeSession;
        if (!id) {
          return;
        }
        void (async () => {
          try {
            sessions = await removeDictationTranscript(id);
            if (selectedSessionId === id) {
              selectedSessionId = null;
            }
            selectBestSession();
            paint();
          } catch (error) {
            console.error("remove dictation transcript", error);
          }
        })();
      });
    });

    root.querySelectorAll<HTMLButtonElement>("[data-remove-voice-file]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.dataset.removeVoiceFile;
        if (!id) {
          return;
        }
        void (async () => {
          try {
            await removeVoiceIndexEntry(id);
            if (selectedVoiceFileId === id) {
              selectedVoiceFileId = null;
              if (activePanel === "file") {
                activePanel = "session";
              }
              setVoiceIndexSelection(null);
            }
            await refreshVoiceFiles();
            paint();
          } catch (error) {
            console.error("remove voice index entry", error);
          }
        })();
      });
    });

    root.querySelector("[data-copy-result]")?.addEventListener("click", () => {
      const textarea = root.querySelector<HTMLTextAreaElement>("[data-result-text]");
      const text = textarea?.value.trim() ?? "";
      if (!text) {
        return;
      }
      void (async () => {
        try {
          await copyTextToClipboard(text);
          copyHint = t("tools.dictationTranscripts.copied");
        } catch {
          copyHint = t("tools.dictationTranscripts.copyFailed");
        }
        paint();
        if (copyHintTimer !== null) {
          window.clearTimeout(copyHintTimer);
        }
        copyHintTimer = window.setTimeout(() => {
          copyHint = null;
          paint();
        }, 1600);
      })();
    });
  };

  void refresh();

  indexUnsubscribe = subscribeVoiceIndex(
    () => {
      void refreshVoiceFiles().then(paint);
    },
    (id) => {
      void refreshVoiceFiles().then(() => {
        if (!id || !voiceFiles.some((e) => e.id === id)) {
          return;
        }
        selectedVoiceFileId = id;
        activePanel = "file";
        paint();
      });
    },
  );

  void listen<DictationTranscriptEntry>(EVENTS.dictationTranscriptUpdated, (event) => {
    const updated = event.payload;
    const idx = sessions.findIndex((e) => e.id === updated.id);
    if (idx >= 0) {
      sessions[idx] = updated;
    } else {
      sessions.unshift(updated);
    }
    sessions.sort((a, b) => b.updatedAtMs - a.updatedAtMs);
    if (updated.status === "active") {
      selectedSessionId = updated.id;
      activePanel = "session";
    } else if (!selectedSessionId) {
      selectedSessionId = updated.id;
    }
    paint();
  }).then((unlisten) => {
    unlisteners.push(unlisten);
  });

  window.addEventListener("beforeunload", () => {
    indexUnsubscribe?.();
    for (const unlisten of unlisteners) {
      unlisten();
    }
  });
}
