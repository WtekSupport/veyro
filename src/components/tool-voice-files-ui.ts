import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  defaultVoiceWatchSettings,
  enqueueVoiceFiles,
  EVENTS,
  getSettings,
  getVoiceQueue,
  listVoiceHistory,
  pickVoiceFiles,
  pickVoiceWatchFolder,
  removeVoiceIndexEntry,
  revealVoiceSource,
  retryVoiceHistoryEntry,
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
  iconSidebarCollapse,
  iconSidebarExpand,
  iconTrash,
} from "./icons";
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import { formatToolErrorForResultField } from "../lib/tool-error-display";
import { escapeHtml, pathsMatch } from "../lib/tool-file-queue";
import { promptSttLanguageSelection } from "./stt-language-dialog";
import { t } from "../i18n";

type IndexEntry = {
  id: string;
  path: string;
  fileName: string;
  status: string;
  text: string;
  errorKey: string | null;
  processing: boolean;
};

function stageLabel(phase: VoiceFileProgressPhase | null): string {
  switch (phase) {
    case "decoding":
      return t("tools.voiceFiles.stage.decoding");
    case "transcribing":
      return t("tools.voiceFiles.stage.transcribing");
    case "text_cleanup":
      return t("tools.voiceFiles.stage.textCleanup");
    case "ai_rewrite":
      return t("tools.voiceFiles.stage.aiRewrite");
    case "done":
    case null:
      return t("tools.voiceFiles.status.processing");
  }
}

function statusLabelFor(
  status: string,
  phase: VoiceFileProgressPhase | null,
  percent: number | null,
): string {
  switch (status) {
    case "pending":
      return t("tools.voiceFiles.status.pending");
    case "processing": {
      const stage = stageLabel(phase);
      if (percent !== null && percent >= 0) {
        return `${stage} ${percent}%`;
      }
      return stage;
    }
    case "done":
      return t("tools.voiceFiles.status.done");
    case "speech_unrecognized":
      return t("tools.voiceWatch.status.speechUnrecognized");
    case "too_long":
      return t("tools.voiceWatch.status.tooLong");
    case "not_audio":
      return t("tools.voiceWatch.status.notAudio");
    case "skipped":
      return t("tools.voiceWatch.status.skipped");
    case "error":
      return t("tools.voiceFiles.status.error");
    default:
      return status;
  }
}

/** Merge persisted history (index) with live queue jobs — real paths + live status. */
function buildIndex(
  history: VoiceHistoryEntry[],
  jobs: VoiceJob[],
): IndexEntry[] {
  const byId = new Map<string, IndexEntry>();
  for (const entry of history) {
    byId.set(entry.id, {
      id: entry.id,
      path: entry.path,
      fileName: entry.fileName,
      status: entry.status,
      text: entry.text,
      errorKey: entry.errorKey ?? null,
      processing: entry.status === "processing",
    });
  }
  for (const job of jobs) {
    const prev = byId.get(job.id);
    byId.set(job.id, {
      id: job.id,
      path: job.path,
      fileName: job.fileName,
      status: job.status,
      text: job.text || prev?.text || "",
      errorKey: job.errorKey ?? prev?.errorKey ?? null,
      processing: job.status === "processing",
    });
  }
  return Array.from(byId.values());
}

function renderFileBrowser(
  entries: IndexEntry[],
  selectedId: string | null,
  progressByPath: Map<string, { phase: VoiceFileProgressPhase; percent: number | null }>,
  collapsed: boolean,
  processing: boolean,
): string {
  if (collapsed) {
    return `
      <aside class="voice-files-sidebar voice-files-sidebar--collapsed" aria-label="${escapeHtml(t("tools.voiceFiles.sidebarTitle"))}">
        <button type="button" class="icon-btn voice-files-sidebar-toggle" data-toggle-sidebar
          aria-label="${escapeHtml(t("tools.voiceFiles.sidebarExpand"))}"
          title="${escapeHtml(t("tools.voiceFiles.sidebarExpand"))}">${iconSidebarExpand()}</button>
      </aside>`;
  }

  const rows =
    entries.length === 0
      ? `<li class="voice-files-browser-empty muted">${escapeHtml(t("tools.voiceFiles.indexEmpty"))}</li>`
      : entries
          .map((entry) => {
            const selected = entry.id === selectedId;
            const prog = progressByPath.get(entry.path);
            const statusText = statusLabelFor(
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

  return `
    <aside class="voice-files-sidebar" aria-label="${escapeHtml(t("tools.voiceFiles.sidebarTitle"))}">
      <div class="voice-files-sidebar-head">
        <span class="voice-files-sidebar-title">${escapeHtml(t("tools.voiceFiles.sidebarTitle"))}</span>
        <button type="button" class="icon-btn" data-toggle-sidebar
          aria-label="${escapeHtml(t("tools.voiceFiles.sidebarCollapse"))}"
          title="${escapeHtml(t("tools.voiceFiles.sidebarCollapse"))}">${iconSidebarCollapse()}</button>
      </div>
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
    </aside>`;
}

function renderWatchPanel(
  watch: VoiceWatchSettings,
  expert: boolean,
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
    <section class="voice-watch-panel" aria-label="${escapeHtml(t("tools.voiceWatch.title"))}">
      <div class="voice-watch-header">
        <label class="voice-watch-toggle">
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
        </div>
      </div>
      <p class="voice-watch-hint">${escapeHtml(t("tools.voiceWatch.howTo"))}</p>
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
      ${
        expert
          ? `
        <details class="voice-watch-expert">
          <summary>${escapeHtml(t("tools.voiceWatch.expertOptions"))}</summary>
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
          <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.notify"))}</span>
            <select data-watch-notify>
              <option value="off" ${watch.notify === "off" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.off"))}</option>
              <option value="result" ${watch.notify === "result" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.result"))}</option>
              <option value="result_and_copy" ${watch.notify === "result_and_copy" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.resultAndCopy"))}</option>
            </select>
          </label>
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
        </details>
      `
          : `
        <label class="field-row"><span>${escapeHtml(t("tools.voiceWatch.notify"))}</span>
          <select data-watch-notify>
            <option value="off" ${watch.notify === "off" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.off"))}</option>
            <option value="result" ${watch.notify === "result" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.result"))}</option>
            <option value="result_and_copy" ${watch.notify === "result_and_copy" ? "selected" : ""}>${escapeHtml(t("tools.voiceWatch.notify.resultAndCopy"))}</option>
          </select>
        </label>
      `
      }
    </section>
  `;
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
    sidebarCollapsed,
    history,
    selectedHistoryId,
  } = args;

  const index = buildIndex(history, jobs);
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
    displayText = `${stageLabel(prog?.phase ?? null)} — ${selectedJob.fileName}`;
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
    displayText = statusLabelFor(selectedEntry.status, null, null);
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
  const copyTitle = copyHint ?? t("tools.voiceFiles.copy");
  const showRetryLang =
    selectedEntry?.status === "speech_unrecognized" ||
    selectedEntry?.errorKey === "tools.stt.selectLanguage";

  return `
    <main class="voice-files-tool voice-files-tool--with-browser${sidebarCollapsed ? " voice-files-tool--sidebar-collapsed" : ""}">
      ${renderFileBrowser(index, selectedEntry?.id ?? null, progressByPath, sidebarCollapsed, processing)}
      <div class="voice-files-main">
        ${renderWatchPanel(watch, expert, cloudPaused, foldersOpen)}

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
                  <button type="button" class="btn btn-secondary btn-compact" data-reveal-source>${escapeHtml(t("tools.voiceFiles.showInFolder"))}</button>
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
  let sidebarCollapsed = false;
  const progressByPath = new Map<
    string,
    { phase: VoiceFileProgressPhase; percent: number | null }
  >();
  const decodeProgress = new ToolDecodeProgressSmoother();
  const unlistens: UnlistenFn[] = [];

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
    const entry = buildIndex(history, jobs).find((e) => e.id === id);
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
    copyHint = null;
    paint();
  };

  const bind = (): void => {
    root.querySelector("[data-toggle-sidebar]")?.addEventListener("click", () => {
      sidebarCollapsed = !sidebarCollapsed;
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

    root.querySelector("[data-reveal-source]")?.addEventListener("click", () => {
      const entry = buildIndex(history, jobs).find(
        (e) => e.id === selectedHistoryId || e.id === selectedId,
      );
      if (entry?.path) void revealVoiceSource(entry.path);
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
      await listen(EVENTS.voiceQueueChanged, () => {
        void refreshQueue().then(paint);
      }),
    );
    unlistens.push(
      await listen(EVENTS.voiceHistoryChanged, () => {
        void refreshHistory().then(paint);
      }),
    );
    unlistens.push(
      await listen<string>(EVENTS.voiceHistoryOpen, (event) => {
        selectedHistoryId = event.payload;
        selectedId = event.payload;
        sidebarCollapsed = false;
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
      for (const u of unlistens) void u();
      decodeProgress.dispose();
    },
  };
}
