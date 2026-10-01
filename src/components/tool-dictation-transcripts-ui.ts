import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  EVENTS,
  listDictationTranscripts,
  removeDictationTranscript,
  type DictationTranscriptEntry,
} from "../api";
import { iconCopy, iconSidebarCollapse, iconSidebarExpand } from "./icons";
import { escapeHtml } from "../lib/html";
import { t } from "../i18n";

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

function renderSidebar(
  entries: DictationTranscriptEntry[],
  selectedId: string | null,
  collapsed: boolean,
): string {
  if (collapsed) {
    return `
      <aside class="voice-files-sidebar voice-files-sidebar--collapsed" aria-label="${escapeHtml(t("tools.dictationTranscripts.sidebarTitle"))}">
        <button type="button" class="icon-btn voice-files-sidebar-toggle" data-toggle-sidebar
          aria-label="${escapeHtml(t("tools.dictationTranscripts.sidebarExpand"))}"
          title="${escapeHtml(t("tools.dictationTranscripts.sidebarExpand"))}">${iconSidebarExpand()}</button>
      </aside>`;
  }

  const rows =
    entries.length === 0
      ? `<li class="voice-files-browser-empty muted">${escapeHtml(t("tools.dictationTranscripts.emptyList"))}</li>`
      : entries
          .map((entry) => {
            const selected = entry.id === selectedId;
            return `
            <li class="voice-files-browser-row${selected ? " voice-files-browser-row--selected" : ""} voice-files-browser-row--${escapeHtml(entry.status)}"
                data-index-id="${escapeHtml(entry.id)}">
              <button type="button" class="voice-files-browser-main" data-select-index="${escapeHtml(entry.id)}">
                <span class="voice-files-browser-name">${escapeHtml(formatSessionLabel(entry))}</span>
                <span class="voice-files-browser-status">${escapeHtml(statusLabel(entry.status))} · ${escapeHtml(previewLine(entry.text))}</span>
              </button>
              <button type="button" class="voice-files-browser-remove" data-remove-index="${escapeHtml(entry.id)}"
                aria-label="${escapeHtml(t("tools.dictationTranscripts.delete"))}"
                title="${escapeHtml(t("tools.dictationTranscripts.delete"))}">×</button>
            </li>`;
          })
          .join("");

  return `
    <aside class="voice-files-sidebar" aria-label="${escapeHtml(t("tools.dictationTranscripts.sidebarTitle"))}">
      <div class="voice-files-sidebar-head">
        <span class="voice-files-sidebar-title">${escapeHtml(t("tools.dictationTranscripts.sidebarTitle"))}</span>
        <button type="button" class="icon-btn" data-toggle-sidebar
          aria-label="${escapeHtml(t("tools.dictationTranscripts.sidebarCollapse"))}"
          title="${escapeHtml(t("tools.dictationTranscripts.sidebarCollapse"))}">${iconSidebarCollapse()}</button>
      </div>
      <ul class="voice-files-browser" role="list">${rows}</ul>
    </aside>`;
}

function renderTool(
  entries: DictationTranscriptEntry[],
  selectedId: string | null,
  copyHint: string | null,
  sidebarCollapsed: boolean,
): string {
  const selected =
    entries.find((e) => e.id === selectedId) ??
    entries.find((e) => e.status === "active") ??
    entries[0] ??
    null;
  const displayText = selected?.text ?? "";
  const canCopy = Boolean(displayText.trim());
  const copyTitle = copyHint ?? t("tools.dictationTranscripts.copy");

  return `
    <div class="voice-files-tool voice-files-tool--with-browser${sidebarCollapsed ? " voice-files-tool--sidebar-collapsed" : ""}">
      ${renderSidebar(entries, selected?.id ?? null, sidebarCollapsed)}
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
          ${
            selected
              ? `<p class="voice-files-stage-hint">${escapeHtml(statusLabel(selected.status))} · ${escapeHtml(formatSessionLabel(selected))}</p>`
              : ""
          }
          ${copyHint ? `<p class="voice-files-copy-hint">${escapeHtml(copyHint)}</p>` : ""}
        </div>
      </div>
    </div>
  `;
}

export function createDictationTranscriptsController(root: HTMLElement): void {
  let entries: DictationTranscriptEntry[] = [];
  let selectedId: string | null = null;
  let copyHint: string | null = null;
  let sidebarCollapsed = false;
  let copyHintTimer: number | null = null;
  const unlisteners: UnlistenFn[] = [];

  const paint = (): void => {
    root.innerHTML = renderTool(entries, selectedId, copyHint, sidebarCollapsed);
    bind();
  };

  const selectBest = (): void => {
    if (selectedId && entries.some((e) => e.id === selectedId)) {
      return;
    }
    selectedId =
      entries.find((e) => e.status === "active")?.id ?? entries[0]?.id ?? null;
  };

  const refresh = async (): Promise<void> => {
    try {
      entries = await listDictationTranscripts();
    } catch (error) {
      console.error("list dictation transcripts", error);
      entries = [];
    }
    selectBest();
    paint();
  };

  const bind = (): void => {
    root.querySelector("[data-toggle-sidebar]")?.addEventListener("click", () => {
      sidebarCollapsed = !sidebarCollapsed;
      paint();
    });

    root.querySelectorAll<HTMLButtonElement>("[data-select-index]").forEach((btn) => {
      btn.addEventListener("click", () => {
        selectedId = btn.dataset.selectIndex ?? null;
        paint();
      });
    });

    root.querySelectorAll<HTMLButtonElement>("[data-remove-index]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.dataset.removeIndex;
        if (!id) {
          return;
        }
        void (async () => {
          try {
            entries = await removeDictationTranscript(id);
            if (selectedId === id) {
              selectedId = null;
            }
            selectBest();
            paint();
          } catch (error) {
            console.error("remove dictation transcript", error);
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

  void listen<DictationTranscriptEntry>(EVENTS.dictationTranscriptUpdated, (event) => {
    const updated = event.payload;
    const idx = entries.findIndex((e) => e.id === updated.id);
    if (idx >= 0) {
      entries[idx] = updated;
    } else {
      entries.unshift(updated);
    }
    entries.sort((a, b) => b.updatedAtMs - a.updatedAtMs);
    if (updated.status === "active") {
      selectedId = updated.id;
    } else if (!selectedId) {
      selectedId = updated.id;
    }
    paint();
  }).then((unlisten) => {
    unlisteners.push(unlisten);
  });

  window.addEventListener("beforeunload", () => {
    for (const unlisten of unlisteners) {
      unlisten();
    }
  });
}
