import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  defaultVoiceWatchSettings,
  enqueueVoiceFiles,
  EVENTS,
  getSettings,
  getVoiceQueue,
  listVoiceHistory,
  pickVoiceDocxSavePath,
  pickVoiceFiles,
  pickVoiceWatchFolder,
  removeVoiceIndexEntry,
  retryVoiceHistoryEntry,
  saveVoiceResultDocx,
  updateSettings,
  voiceWatchSetEnabled,
  voiceWatchUsesCloud,
  type AppSettings,
  type VoiceFileProgressPayload,
  type VoiceFileProgressPhase,
  type VoiceHistoryEntry,
  type VoiceJob,
  type VoiceWatchSettings,
} from "../api";
import {
  iconCopy,
  iconFolder,
  iconList,
  iconSettings,
  iconTrash,
} from "./icons";
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import { formatToolErrorForResultField } from "../lib/tool-error-display";
import { escapeHtml } from "../lib/html";
import { pathsMatch } from "../lib/tool-file-queue";
import {
  loadToolBrowserSidebarCollapsed,
  renderToolBrowserSidebar,
  saveToolBrowserSidebarCollapsed,
  toolBrowserShellClass,
  type ToolBrowserSidebarLabels,
} from "../lib/tool-browser-sidebar";
import {
  buildVoiceIndex,
  getVoiceIndexSelection,
  setVoiceIndexSelection,
  subscribeVoiceIndex,
  voiceIndexStatusLabel,
  type VoiceIndexEntry,
} from "../lib/tool-voice-index";
import { promptSttLanguageSelection } from "./stt-language-dialog";
import { t } from "../i18n";

function renderExpertFields(watch: VoiceWatchSettings, expert: boolean): string {
  const notifyField = `
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.notify"))}</span>
      <select data-watch-notify>
        <option value="off" ${watch.notify === "off" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.off"))}</option>
        <option value="result" ${watch.notify === "result" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.result"))}</option>
        <option value="result_and_copy" ${watch.notify === "result_and_copy" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.resultAndCopy"))}</option>
      </select>
    </label>`;

  if (!expert) {
    return notifyField;
  }

  return `
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.recursive"))}</span>
      <input type="checkbox" data-watch-recursive ${watch.recursive ? "checked" : ""} />
    </label>
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.extensions"))}</span>
      <input type="text" data-watch-extensions value="${escapeHtml(watch.extensions.join(", "))}" />
    </label>
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.textMode"))}</span>
      <select data-watch-text-mode>
        <option value="inherit" ${watch.text_mode_override === "inherit" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.textMode.inherit"))}</option>
        <option value="original" ${watch.text_mode_override === "original" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.textMode.original"))}</option>
        <option value="basic" ${watch.text_mode_override === "basic" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.textMode.basic"))}</option>
      </select>
    </label>
    ${notifyField}
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.onlyLocal"))}</span>
      <input type="checkbox" data-watch-only-local ${watch.only_local_providers ? "checked" : ""} />
    </label>
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.deleteSource"))}</span>
      <input type="checkbox" data-watch-delete-source ${watch.delete_source_after ? "checked" : ""} />
    </label>
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.moveSource"))}</span>
      <input type="checkbox" data-watch-move-source ${watch.move_source_after ? "checked" : ""} />
    </label>
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.retention"))}</span>
      <input type="number" min="1" max="365" data-watch-retention value="${watch.history_retention_days}" />
    </label>
    <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.nameFilter"))}</span>
      <input type="text" data-watch-name-filter value="${escapeHtml(watch.name_filter)}" placeholder="audio_*" />
    </label>
  `;
}

function renderExpertDialog(watch: VoiceWatchSettings, expert: boolean): string {
  return `
    <div class="confirm-overlay voice-watch-expert-overlay" data-expert-overlay>
      <div class="confirm-dialog voice-watch-expert-dialog" role="dialog" aria-modal="true" aria-labelledby="voice-watch-expert-title">
        <h2 id="voice-watch-expert-title" class="confirm-dialog-title">${escapeHtml(t("tools.voiceWatch.expertOptions"))}</h2>
        <div class="voice-watch-expert-body">
          ${renderExpertFields(watch, expert)}
        </div>
        <div class="confirm-dialog-actions">
          <button type="button" class="btn btn-primary" data-close-expert>
            ${escapeHtml(t("tools.voiceWatch.expertClose"))}
          </button>
        </div>
      </div>
    </div>
  `;
}

function renderWatchCompact(
  watch: VoiceWatchSettings,
  cloudPaused: boolean,
  foldersOpen: boolean,
): string {
  const folders = watch.folders
    .map(
      (folder, index) => `
      <li class="voice-watch-folder">
        <span class="voice-watch-folder-path" title="${escapeHtml(folder)}">${escapeHtml(folder)}</span>
        <button type="button" class="icon-btn voice-watch-folder-remove" data-remove-folder="${index}"
          aria-label="${escapeHtml(t("tools.voiceWatch.removeFolder"))}"
          title="${escapeHtml(t("tools.voiceWatch.removeFolder"))}">${iconTrash()}</button>
      </li>`,
    )
    .join("");

  return `
    <section class="voice-watch-panel voice-watch-panel--sidebar" aria-label="${escapeHtml(t("tools.voiceWatch.title"))}">
      <div class="voice-watch-header">
        <label class="voice-watch-toggle" title="${escapeHtml(t("tools.voiceWatch.howTo"))}">
          <input type="checkbox" data-voice-watch-enabled ${watch.enabled ? "checked" : ""} />
          <span>${escapeHtml(t("tools.voiceWatch.enable"))}</span>
        </label>
        <div class="voice-watch-folder-actions">
          <button type="button" class="icon-btn" data-pick-folder
            aria-label="${escapeHtml(t("tools.voiceWatch.chooseFolder"))}"
            title="${escapeHtml(t("tools.voiceWatch.chooseFolder"))}">${iconFolder()}</button>
          <button type="button" class="icon-btn${foldersOpen ? " icon-btn--active" : ""}" data-toggle-folders
            aria-expanded="${foldersOpen}"
            aria-label="${escapeHtml(t("tools.voiceWatch.listFolders"))}"
            title="${escapeHtml(t("tools.voiceWatch.listFolders"))}">${iconList()}</button>
          <button type="button" class="icon-btn" data-open-expert
            aria-label="${escapeHtml(t("tools.voiceWatch.expertOptions"))}"
            title="${escapeHtml(t("tools.voiceWatch.expertOptions"))}">${iconSettings()}</button>
        </div>
      </div>
      ${cloudPaused ? `<p class="voice-watch-warn">${escapeHtml(t("tools.voiceWatch.pausedCloud"))}</p>` : ""}
      ${
        watch.enabled && watch.folders.length === 0
          ? `<p class="voice-watch-warn">${escapeHtml(t("tools.voiceWatch.noFolders"))}</p>`
          : ""
      }
      ${
        foldersOpen
          ? `<ul class="voice-watch-folders">${folders || `<li class="muted">${escapeHtml(t("tools.voiceWatch.foldersEmpty"))}</li>`}</ul>`
          : ""
      }
    </section>
  `;
}

const VOICE_FILES_SIDEBAR_TOOL_ID = "voice-files";

function voiceFilesSidebarLabels(): ToolBrowserSidebarLabels {
  return {
    ariaLabel: t("tools.voiceFiles.sidebarTitle"),
    title: t("tools.voiceFiles.sidebarTitle"),
    collapseLabel: t("tools.voiceFiles.sidebarCollapse"),
    expandLabel: t("tools.voiceFiles.sidebarExpand"),
  };
}

function renderFileBrowser(
  entries: VoiceIndexEntry[],
  selectedId: string | null,
  progressByPath: Map<string, { phase: VoiceFileProgressPhase; percent: number | null }>,
  collapsed: boolean,
  processing: boolean,
  watch: VoiceWatchSettings,
  cloudPaused: boolean,
  foldersOpen: boolean,
): string {
  const rows =
    entries.length === 0
      ? `<li class="voice-files-browser-empty muted">${escapeHtml(t("tools.voiceFiles.indexEmpty"))}</li>`
      : entries
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
                data-index-id="${escapeHtml(entry.id)}"
                data-index-path="${escapeHtml(entry.path)}"
                title="${escapeHtml(entry.path)}">
              <button type="button" class="voice-files-browser-main" data-select-index="${escapeHtml(entry.id)}">
                <span class="voice-files-browser-name">${escapeHtml(entry.fileName)}</span>
                <span class="voice-files-browser-status">${escapeHtml(statusText)}</span>
              </button>
              <button type="button" class="voice-files-browser-remove" data-remove-index="${escapeHtml(entry.id)}"
                ${canRemove ? "" : "disabled"}
                aria-label="${escapeHtml(t("tools.voiceWatch.removeFromIndex"))}"
                title="${escapeHtml(t("tools.voiceWatch.removeFromIndex"))}">×</button>
            </li>`;
          })
          .join("");

  const body = `
      <div
        class="voice-files-dropzone voice-files-dropzone--sidebar${processing ? " voice-files-dropzone--busy" : ""}"
        data-voice-files-dropzone
        tabindex="0"
        role="region"
        aria-label="${escapeHtml(t("tools.voiceFiles.pickFiles"))}"
      >
        <button type="button" class="btn btn-secondary btn-compact" data-pick-voice-files ${processing ? "disabled" : ""}>
          ${escapeHtml(t("tools.voiceFiles.pickFiles"))}
        </button>
      </div>
      <ul class="voice-files-browser" role="list">${rows}</ul>
      ${renderWatchCompact(watch, cloudPaused, foldersOpen)}`;

  return renderToolBrowserSidebar(voiceFilesSidebarLabels(), collapsed, body);
}
function renderTool(args: {
  jobs: VoiceJob[];
  selectedId: string | null;
  copyHint: string | null;
  progressByPath: Map<string, { phase: VoiceFileProgressPhase; percent: number | null }>;
  visiblePercent: number | null;
  watch: VoiceWatchSettings;
  expert: boolean;
  cloudPaused: boolean;
  foldersOpen: boolean;
  expertOpen: boolean;
  sidebarCollapsed: boolean;
  history: VoiceHistoryEntry[];
  selectedHistoryId: string | null;
}): string {
  const {
    jobs,
    selectedId,
    copyHint,
    progressByPath,
    visiblePercent,
    watch,
    expert,
    cloudPaused,
    foldersOpen,
    expertOpen,
    sidebarCollapsed,
    history,
    selectedHistoryId,
  } = args;

  const index = buildVoiceIndex(history, jobs);
  const selectedEntry =
    index.find((e) => e.id === selectedHistoryId) ??
    index.find((e) => e.id === selectedId) ??
    null;
  const selectedJob = jobs.find((j) => j.id === selectedEntry?.id) ?? null;

  let displayText = "";
  let isError = false;
  let isInfo = false;
  if (selectedEntry?.text.trim()) {
    displayText = selectedEntry.text;
    isError = selectedEntry.status === "error";
    isInfo = !isError && selectedEntry.status !== "done";
  } else if (selectedJob?.status === "processing") {
    const prog = progressByPath.get(selectedJob.path);
    displayText = `${voiceIndexStatusLabel("processing", prog?.phase ?? null, null)} — ${selectedJob.fileName}`;
    isInfo = true;
  } else if (selectedEntry?.status === "pending") {
    displayText = `${t("tools.voiceFiles.status.pending")} ${selectedEntry.fileName}`;
    isInfo = true;
  } else if (selectedEntry?.errorKey) {
    displayText = formatToolErrorForResultField(
      selectedEntry.errorKey,
      "tools.voiceFiles.failed",
    );
    isError = selectedEntry.status === "error";
    isInfo = !isError;
  } else if (selectedEntry) {
    displayText = voiceIndexStatusLabel(selectedEntry.status, null, null);
    isInfo = true;
  }

  const processing = jobs.some((j) => j.status === "processing");
  const activeProg = selectedJob ? progressByPath.get(selectedJob.path) : undefined;
  const progressPercent =
    selectedJob?.status === "processing"
      ? (visiblePercent ?? activeProg?.percent ?? null)
      : null;
  const canCopy =
    Boolean(displayText.trim()) && selectedEntry?.status !== "pending";
  const canSaveDocx =
    Boolean(displayText.trim()) &&
    !isError &&
    selectedEntry?.status === "done";
  const copyTitle = copyHint ?? t("tools.voiceFiles.copy");
  const showRetryLang =
    selectedEntry?.status === "speech_unrecognized" ||
    selectedEntry?.errorKey === "tools.stt.selectLanguage";

  return `
    <main class="${toolBrowserShellClass(sidebarCollapsed)}">
      ${renderFileBrowser(
        index,
        selectedEntry?.id ?? null,
        progressByPath,
        sidebarCollapsed,
        processing,
        watch,
        cloudPaused,
        foldersOpen,
      )}
      <div class="voice-files-main">
        ${
          progressPercent !== null
            ? `
          <div class="voice-files-progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${progressPercent}">
            <div class="voice-files-progress-fill" style="width: ${progressPercent}%"></div>
          </div>`
            : ""
        }

        <div class="voice-files-text-stack" aria-live="polite">
          <div class="voice-files-text-wrap">
            <button type="button" class="voice-files-copy-overlay icon-btn" data-copy-voice-result
              ${canCopy ? "" : "disabled"} aria-label="${escapeHtml(copyTitle)}" title="${escapeHtml(copyTitle)}">${iconCopy()}</button>
            <textarea class="voice-files-textarea${isError ? " voice-files-textarea--error" : isInfo ? " voice-files-textarea--info" : ""}"
              readonly data-voice-result-text aria-label="${escapeHtml(t("tools.voiceFiles.resultTitle"))}">${escapeHtml(displayText)}</textarea>
          </div>
          ${
            selectedEntry
              ? `<div class="voice-history-item-actions">
                  <button type="button" class="btn btn-secondary btn-compact" data-save-docx ${canSaveDocx ? "" : "disabled"}>${escapeHtml(t("tools.voiceFiles.saveDocx"))}</button>
                  <button type="button" class="btn btn-secondary btn-compact" data-retry-history>${escapeHtml(t("tools.voiceWatch.retry"))}</button>
                  ${showRetryLang ? `<button type="button" class="btn btn-secondary btn-compact" data-retry-with-lang>${escapeHtml(t("tools.voiceWatch.retryWithLang"))}</button>` : ""}
                  <button type="button" class="btn btn-ghost btn-compact" data-remove-index-selected
                    ${selectedEntry.processing ? "disabled" : ""}
                    title="${escapeHtml(t("tools.voiceWatch.removeFromIndex"))}">${escapeHtml(t("tools.voiceWatch.removeFromIndex"))}</button>
                </div>`
              : ""
          }
        </div>
      </div>
      ${expertOpen ? renderExpertDialog(watch, expert) : ""}
    </main>
  `;
}
export function createVoiceFilesController(root: HTMLElement): {
  enqueuePaths: (paths: string[]) => void;
  dispose: () => void;
} {
  let jobs: VoiceJob[] = [];
  let history: VoiceHistoryEntry[] = [];
  let selectedId: string | null = null;
  let selectedHistoryId: string | null = null;
  let copyHint: string | null = null;
  let watch = defaultVoiceWatchSettings();
  let expert = false;
  let cloudPaused = false;
  let foldersOpen = false;
  let expertOpen = false;
  let sidebarCollapsed = loadToolBrowserSidebarCollapsed(VOICE_FILES_SIDEBAR_TOOL_ID);
  let expertKeyHandler: ((event: KeyboardEvent) => void) | null = null;
  const progressByPath = new Map<
    string,
    { phase: VoiceFileProgressPhase; percent: number | null }
  >();
  const decodeProgress = new ToolDecodeProgressSmoother();
  const unlistens: UnlistenFn[] = [];

  const clearExpertKeyHandler = (): void => {
    if (expertKeyHandler) {
      document.removeEventListener("keydown", expertKeyHandler);
      expertKeyHandler = null;
    }
  };
  const paint = (): void => {
    const selected = jobs.find((j) => j.id === selectedId);
    const prog = selected ? progressByPath.get(selected.path) : undefined;
    const visible = decodeProgress.displayPercent(
      prog?.phase ?? null,
      prog?.percent ?? null,
    );
    root.innerHTML = renderTool({
      jobs,
      selectedId,
      copyHint,
      progressByPath,
      visiblePercent: visible,
      watch,
      expert,
      cloudPaused,
      foldersOpen,
      expertOpen,
      sidebarCollapsed,
      history,
      selectedHistoryId,
    });
    bind();
  };

  decodeProgress.bindRepaint(paint);

  const persistWatch = async (next: VoiceWatchSettings): Promise<void> => {
    watch = next;
    paint();
    try {
      await updateSettings({ voice_watch: next });
      const statusCloud = await voiceWatchUsesCloud();
      cloudPaused = Boolean(next.enabled && next.only_local_providers && statusCloud);
      paint();
    } catch {
      /* keep UI */
    }
  };

  const refreshSettings = async (): Promise<void> => {
    try {
      const settings: AppSettings = await getSettings();
      watch = settings.voice_watch ?? defaultVoiceWatchSettings();
      expert = settings.ui_mode === "expert";
      cloudPaused = Boolean(
        watch.enabled &&
          watch.only_local_providers &&
          (await voiceWatchUsesCloud()),
      );
    } catch {
      /* ignore */
    }
  };

  const refreshQueue = async (): Promise<void> => {
    try {
      jobs = (await getVoiceQueue()).jobs;
    } catch {
      /* ignore */
    }
  };

  const refreshHistory = async (): Promise<void> => {
    try {
      history = await listVoiceHistory();
    } catch {
      /* ignore */
    }
  };

  const removeFromIndex = async (id: string): Promise<void> => {
    const entry = buildVoiceIndex(history, jobs).find((e) => e.id === id);
    if (!entry || entry.processing) {
      return;
    }
    try {
      history = await removeVoiceIndexEntry(id);
      if (selectedHistoryId === id || selectedId === id) {
        selectedHistoryId = null;
        selectedId = null;
      }
      await refreshQueue();
      paint();
    } catch {
      /* ignore */
    }
  };

  const enqueuePaths = (paths: string[]): void => {
    const unique = paths.filter((p) => p.trim().length > 0);
    if (unique.length === 0) {
      return;
    }
    void enqueueVoiceFiles(unique, "manual").then((created) => {
      if (created[0]) {
        selectedId = created[0].id;
        selectedHistoryId = created[0].id;
        setVoiceIndexSelection(created[0].id);
      }
      void refreshQueue().then(paint);
      void refreshHistory().then(paint);
    });
  };

  const bindSwipeToRemove = (): void => {
    root.querySelectorAll<HTMLElement>(".voice-files-browser-row").forEach((row) => {
      let startX = 0;
      let dragging = false;
      let swiping = false;
      row.addEventListener("pointerdown", (event) => {
        if ((event.target as HTMLElement).closest("[data-remove-index]")) {
          return;
        }
        startX = event.clientX;
        dragging = true;
        swiping = false;
      });
      row.addEventListener("pointermove", (event) => {
        if (!dragging) return;
        const dx = event.clientX - startX;
        if (!swiping && Math.abs(dx) < 10) return;
        if (!swiping) {
          swiping = true;
          row.setPointerCapture(event.pointerId);
        }
        const offset = Math.min(0, Math.max(-96, dx));
        row.style.transform = `translateX(${offset}px)`;
        row.classList.toggle("voice-files-browser-row--swipe", offset < -24);
      });
      const endDrag = (event: PointerEvent): void => {
        if (!dragging) return;
        dragging = false;
        const dx = event.clientX - startX;
        row.style.transform = "";
        row.classList.remove("voice-files-browser-row--swipe");
        if (swiping && dx < -72) {
          const id = row.dataset.indexId;
          if (id) void removeFromIndex(id);
        }
        swiping = false;
      };
      row.addEventListener("pointerup", endDrag);
      row.addEventListener("pointercancel", endDrag);
    });
  };

  const selectIndexEntry = (id: string | null): void => {
    if (!id) return;
    selectedId = id;
    selectedHistoryId = id;
    setVoiceIndexSelection(id);
    copyHint = null;
    paint();
  };

  const bind = (): void => {
    root.querySelector("[data-toggle-sidebar]")?.addEventListener("click", () => {
      sidebarCollapsed = !sidebarCollapsed;
      saveToolBrowserSidebarCollapsed(VOICE_FILES_SIDEBAR_TOOL_ID, sidebarCollapsed);
      paint();
    });

    root.querySelectorAll<HTMLElement>(".voice-files-browser-row").forEach((row) => {
      row.addEventListener("click", (event) => {
        if ((event.target as HTMLElement).closest("[data-remove-index]")) return;
        selectIndexEntry(row.dataset.indexId ?? null);
      });
    });

    root.querySelectorAll<HTMLButtonElement>("[data-select-index]").forEach((button) => {
      button.addEventListener("click", (event) => {
        event.stopPropagation();
        selectIndexEntry(button.dataset.selectIndex ?? null);
      });
    });

    root.querySelectorAll<HTMLButtonElement>("[data-remove-index]").forEach((button) => {
      button.addEventListener("click", (event) => {
        event.stopPropagation();
        const id = button.dataset.removeIndex;
        if (id) void removeFromIndex(id);
      });
    });

    bindSwipeToRemove();

    root.querySelector<HTMLButtonElement>("[data-pick-voice-files]")?.addEventListener("click", () => {
      void pickVoiceFiles().then((paths) => {
        if (paths.length) enqueuePaths(paths);
      });
    });

    root.querySelector<HTMLButtonElement>("[data-copy-voice-result]")?.addEventListener("click", () => {
      void (async () => {
        const textarea = root.querySelector<HTMLTextAreaElement>("[data-voice-result-text]");
        const payload = textarea?.value.trim() ?? "";
        if (!payload) return;
        try {
          await copyTextToClipboard(payload);
          copyHint = t("tools.voiceFiles.copied");
        } catch {
          copyHint = t("tools.voiceFiles.copyFailed");
        }
        paint();
        window.setTimeout(() => {
          copyHint = null;
          paint();
        }, 1600);
      })();
    });

    root.querySelector<HTMLInputElement>("[data-voice-watch-enabled]")?.addEventListener("change", (event) => {
      const checked = (event.target as HTMLInputElement).checked;
      void (async () => {
        if (checked) {
          const cloud = await voiceWatchUsesCloud();
          if (cloud && !watch.only_local_providers) {
            const ok = window.confirm(t("tools.voiceWatch.cloudConfirm"));
            if (!ok) {
              paint();
              return;
            }
          }
        }
        try {
          const settings = await voiceWatchSetEnabled(checked);
          watch = settings.voice_watch ?? { ...watch, enabled: checked };
          cloudPaused = Boolean(
            watch.enabled && watch.only_local_providers && (await voiceWatchUsesCloud()),
          );
        } catch {
          watch = { ...watch, enabled: checked };
        }
        paint();
      })();
    });

    root.querySelectorAll<HTMLButtonElement>("[data-remove-folder]").forEach((button) => {
      button.addEventListener("click", () => {
        const index = Number(button.dataset.removeFolder);
        if (!Number.isFinite(index)) return;
        const folders = watch.folders.filter((_, i) => i !== index);
        void persistWatch({ ...watch, folders });
      });
    });

    root.querySelector<HTMLButtonElement>("[data-toggle-folders]")?.addEventListener("click", () => {
      foldersOpen = !foldersOpen;
      paint();
    });

    root.querySelector<HTMLButtonElement>("[data-open-expert]")?.addEventListener("click", () => {
      expertOpen = true;
      paint();
    });

    const closeExpert = (): void => {
      clearExpertKeyHandler();
      expertOpen = false;
      paint();
    };

    root.querySelector<HTMLButtonElement>("[data-close-expert]")?.addEventListener("click", closeExpert);
    root.querySelector<HTMLElement>("[data-expert-overlay]")?.addEventListener("click", (event) => {
      if (event.target === event.currentTarget) {
        closeExpert();
      }
    });
    clearExpertKeyHandler();
    if (expertOpen) {
      expertKeyHandler = (event: KeyboardEvent): void => {
        if (event.key === "Escape") {
          event.preventDefault();
          closeExpert();
        }
      };
      document.addEventListener("keydown", expertKeyHandler);
    }

    root.querySelectorAll<HTMLButtonElement>("[data-pick-folder]").forEach((button) => {
      button.addEventListener("click", () => {
        void pickVoiceWatchFolder().then((path) => {
          if (!path || watch.folders.includes(path)) return;
          foldersOpen = true;
          void persistWatch({ ...watch, folders: [...watch.folders, path] });
        });
      });
    });

    const syncField = (
      selector: string,
      apply: (el: HTMLInputElement | HTMLSelectElement) => VoiceWatchSettings,
    ) => {
      root.querySelector(selector)?.addEventListener("change", (event) => {
        const el = event.target as HTMLInputElement | HTMLSelectElement;
        void persistWatch(apply(el));
      });
    };

    syncField("[data-watch-recursive]", (el) => ({
      ...watch,
      recursive: (el as HTMLInputElement).checked,
    }));
    syncField("[data-watch-only-local]", (el) => ({
      ...watch,
      only_local_providers: (el as HTMLInputElement).checked,
    }));
    syncField("[data-watch-delete-source]", (el) => ({
      ...watch,
      delete_source_after: (el as HTMLInputElement).checked,
    }));
    syncField("[data-watch-move-source]", (el) => ({
      ...watch,
      move_source_after: (el as HTMLInputElement).checked,
    }));
    syncField("[data-watch-notify]", (el) => ({
      ...watch,
      notify: el.value as VoiceWatchSettings["notify"],
    }));
    syncField("[data-watch-text-mode]", (el) => ({
      ...watch,
      text_mode_override: el.value,
    }));
    syncField("[data-watch-extensions]", (el) => ({
      ...watch,
      extensions: el.value
        .split(/[,;\s]+/)
        .map((s) => s.replace(/^\./, "").trim())
        .filter(Boolean),
    }));
    syncField("[data-watch-retention]", (el) => ({
      ...watch,
      history_retention_days: Math.max(1, Number(el.value) || 30),
    }));
    syncField("[data-watch-name-filter]", (el) => ({
      ...watch,
      name_filter: el.value,
    }));

    root.querySelector("[data-save-docx]")?.addEventListener("click", () => {
      void (async () => {
        const entry = buildVoiceIndex(history, jobs).find(
          (e) => e.id === selectedHistoryId || e.id === selectedId,
        );
        const text = entry?.text?.trim() ?? "";
        if (!entry || !text || entry.status !== "done") {
          return;
        }
        const base = entry.fileName.replace(/\.[^.]+$/, "") || "transcript";
        const defaultName = `${base}.docx`;
        try {
          const target = await pickVoiceDocxSavePath(defaultName);
          if (!target) {
            return;
          }
          await saveVoiceResultDocx(target, text);
          copyHint = t("tools.voiceFiles.saveDocxDone");
        } catch {
          copyHint = t("tools.voiceFiles.saveDocxFailed");
        }
        paint();
        window.setTimeout(() => {
          copyHint = null;
          paint();
        }, 1600);
      })();
    });

    root.querySelector("[data-remove-index-selected]")?.addEventListener("click", () => {
      const id = selectedHistoryId ?? selectedId;
      if (id) void removeFromIndex(id);
    });

    root.querySelector("[data-retry-history]")?.addEventListener("click", () => {
      if (!selectedHistoryId) return;
      void retryVoiceHistoryEntry(selectedHistoryId).then((job) => {
        selectedId = job.id;
        selectedHistoryId = job.id;
        void refreshQueue().then(paint);
        void refreshHistory().then(paint);
      });
    });

    root.querySelector("[data-retry-with-lang]")?.addEventListener("click", () => {
      if (!selectedHistoryId) return;
      void promptSttLanguageSelection().then((lang) => {
        if (!lang) return;
        void retryVoiceHistoryEntry(selectedHistoryId!, lang).then((job) => {
          selectedId = job.id;
          selectedHistoryId = job.id;
          void refreshQueue().then(paint);
          void refreshHistory().then(paint);
        });
      });
    });

    const dropzone = root.querySelector<HTMLElement>("[data-voice-files-dropzone]");
    dropzone?.addEventListener("dragover", (event) => {
      event.preventDefault();
      dropzone.classList.add("voice-files-dropzone--hover");
    });
    dropzone?.addEventListener("dragleave", () => {
      dropzone.classList.remove("voice-files-dropzone--hover");
    });
    dropzone?.addEventListener("drop", (event) => {
      event.preventDefault();
      dropzone.classList.remove("voice-files-dropzone--hover");
      const transfer = event.dataTransfer;
      if (!transfer?.files?.length) return;
      const paths: string[] = [];
      for (const file of Array.from(transfer.files)) {
        const withPath = file as File & { path?: string };
        if (withPath.path) paths.push(withPath.path);
      }
      if (paths.length > 0) enqueuePaths(paths);
    });
  };

  void (async () => {
    await refreshSettings();
    await refreshQueue();
    await refreshHistory();
    const persisted = getVoiceIndexSelection();
    if (persisted && buildVoiceIndex(history, jobs).some((e) => e.id === persisted)) {
      selectedId = persisted;
      selectedHistoryId = persisted;
    }
    paint();

    unlistens.push(
      await listen<VoiceFileProgressPayload>(EVENTS.voiceFileProgress, (event) => {
        const payload = event.payload;
        const percent =
          typeof payload.percent === "number" && Number.isFinite(payload.percent)
            ? Math.min(100, Math.max(0, Math.round(payload.percent)))
            : null;
        progressByPath.set(payload.path, { phase: payload.phase, percent });
        const active = jobs.find(
          (j) => j.status === "processing" && pathsMatch(j.path, payload.path),
        );
        decodeProgress.sync(payload.phase, percent);
        if (active) paint();
      }),
    );

    unlistens.push(
      subscribeVoiceIndex(
        () => {
          void refreshQueue().then(() => refreshHistory().then(paint));
        },
        (id) => {
          if (!id) return;
          if (!buildVoiceIndex(history, jobs).some((e) => e.id === id)) return;
          selectedId = id;
          selectedHistoryId = id;
          paint();
        },
      ),
    );
    unlistens.push(
      await listen<string>(EVENTS.voiceHistoryOpen, (event) => {
        selectedHistoryId = event.payload;
        selectedId = event.payload;
        sidebarCollapsed = false;
        saveToolBrowserSidebarCollapsed(VOICE_FILES_SIDEBAR_TOOL_ID, false);
        void refreshHistory().then(paint);
      }),
    );
    unlistens.push(
      await listen(EVENTS.settingsChanged, () => {
        void refreshSettings().then(paint);
      }),
    );
  })();

  return {
    enqueuePaths,
    dispose: (): void => {
      clearExpertKeyHandler();
      for (const u of unlistens) void u();
      decodeProgress.dispose();
    },
  };
}
