import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  EVENTS,
  getSubtitleSttCapability,
  pickSubtitleSavePath,
  pickVoiceFiles,
  saveSubtitleFile,
  transcribeAudioToSrt,
  type AudioSrtOptions,
  type AudioSrtProgressPayload,
  type AudioSrtProgressPhase,
  type AudioSrtTranscriptionResult,
  type SubtitleSttCapability,
} from "../api";
import { createAudioSrtPreview, mountAudioSrtPreview } from "./audio-srt-preview";
import { iconCopy } from "./icons";
import { runToolWithSttLanguageRecovery } from "../lib/tool-stt-auto-recovery";
import { t, type MessageKey } from "../i18n";

export type AudioSrtJobStatus = "pending" | "processing" | "done" | "error";

export interface AudioSrtJob {
  id: string;
  path: string;
  fileName: string;
  status: AudioSrtJobStatus;
  processingPhase: AudioSrtProgressPhase | null;
  processingPercent: number | null;
  srt: string;
  errorKey: string | null;
  rewriteFallback: boolean;
  aiRewriteApplied: boolean;
}

export interface AudioSrtUiOptions {
  maxLineLength: number;
  maxLinesPerCue: number;
  globalOffsetMs: number;
  utf8Bom: boolean;
  useDictationTextSettings: boolean;
  smartSplit: boolean;
  pauseSplitMs: number;
  maxCueDurationMs: number;
  minCueDurationMs: number;
  readingTailMs: number;
  advancedOpen: boolean;
}

const DEFAULT_UI_OPTIONS: AudioSrtUiOptions = {
  maxLineLength: 42,
  maxLinesPerCue: 2,
  globalOffsetMs: 0,
  utf8Bom: false,
  useDictationTextSettings: false,
  smartSplit: true,
  pauseSplitMs: 400,
  maxCueDurationMs: 7000,
  minCueDurationMs: 1000,
  readingTailMs: 200,
  advancedOpen: false,
};

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function fileNameFromPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const segment = normalized.split("/").pop();
  return segment ?? path;
}

function pathsMatch(left: string, right: string): boolean {
  return left.replace(/\\/g, "/").toLowerCase() === right.replace(/\\/g, "/").toLowerCase();
}

function stageLabel(phase: AudioSrtProgressPhase | null): string {
  switch (phase) {
    case "decoding":
      return t("tools.audioSrt.stage.decoding");
    case "transcribing":
      return t("tools.audioSrt.stage.transcribing");
    case "text_cleanup":
      return t("tools.audioSrt.stage.textCleanup");
    case "word_alignment":
      return t("tools.audioSrt.stage.wordAlignment");
    case "generating_subtitles":
      return t("tools.audioSrt.stage.generatingSubtitles");
    case "ai_rewrite":
      return t("tools.audioSrt.stage.aiRewrite");
    case "done":
    case null:
      return t("tools.audioSrt.status.processing");
  }
}

function statusLabel(job: AudioSrtJob): string {
  switch (job.status) {
    case "pending":
      return t("tools.audioSrt.status.pending");
    case "processing": {
      const stage = stageLabel(job.processingPhase);
      if (job.processingPercent !== null && job.processingPercent >= 0) {
        return `${stage} ${job.processingPercent}%`;
      }
      return stage;
    }
    case "done":
      return t("tools.audioSrt.status.done");
    case "error":
      return job.errorKey
        ? t(job.errorKey as MessageKey)
        : t("tools.audioSrt.status.error");
  }
}

function resultFootnote(job: AudioSrtJob | null | undefined): string {
  if (!job || job.status !== "done") {
    return "";
  }
  if (job.rewriteFallback) {
    return t("tools.audioSrt.rewriteFallback");
  }
  if (job.aiRewriteApplied) {
    return t("tools.audioSrt.aiRewriteDone");
  }
  return "";
}

function renderQueueCompact(jobs: AudioSrtJob[], selectedId: string | null): string {
  if (jobs.length === 0) {
    return "";
  }
  return `
    <ul class="voice-files-queue-compact" role="list">
      ${jobs
        .map((job) => {
          const selected = job.id === selectedId;
          const errorText =
            job.errorKey !== null
              ? escapeHtml(t(job.errorKey as MessageKey))
              : "";
          return `
            <li>
              <button
                type="button"
                class="voice-files-queue-chip voice-files-queue-chip--${job.status}${selected ? " voice-files-queue-chip--selected" : ""}"
                data-audio-srt-id="${escapeHtml(job.id)}"
                aria-pressed="${selected}"
                title="${errorText || escapeHtml(job.fileName)}"
              >
                <span class="voice-files-queue-chip-name">${escapeHtml(job.fileName)}</span>
                <span class="voice-files-queue-chip-status">${escapeHtml(statusLabel(job))}</span>
              </button>
            </li>
          `;
        })
        .join("")}
    </ul>
  `;
}

function renderFieldHelp(hintKey: MessageKey): string {
  const hint = escapeHtml(t(hintKey));
  return `
    <span class="field-help-wrap">
      <button
        type="button"
        class="field-help-btn"
        aria-label="${escapeHtml(t("tools.audioSrt.hintShow"))}"
        aria-expanded="false"
      >?</button>
      <span class="field-help-popover" role="tooltip">${hint}</span>
    </span>
  `;
}

function renderFieldTitle(labelKey: MessageKey, hintKey: MessageKey): string {
  return `<span class="field-compact-title">${escapeHtml(t(labelKey))}${renderFieldHelp(hintKey)}</span>`;
}

function renderAdvancedOptions(options: AudioSrtUiOptions): string {
  const pauseSplitField = options.smartSplit
    ? ""
    : `
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.pauseSplitMs", "tools.audioSrt.hint.pauseSplitMs")}
          <input type="number" min="100" max="2000" step="50" data-opt-pause-split value="${options.pauseSplitMs}" />
        </label>
      `;

  return `
    <details class="audio-srt-advanced"${options.advancedOpen ? " open" : ""} data-audio-srt-advanced>
      <summary>${escapeHtml(t("tools.audioSrt.advanced"))}</summary>
      <div class="audio-srt-advanced-body">
        <label class="field-compact field-compact--checkbox">
          <input type="checkbox" data-opt-smart-split ${options.smartSplit ? "checked" : ""} />
          ${renderFieldTitle("tools.audioSrt.smartSplit", "tools.audioSrt.hint.smartSplit")}
        </label>
        ${pauseSplitField}
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.readingTailMs", "tools.audioSrt.hint.readingTailMs")}
          <input type="number" min="0" max="1000" step="50" data-opt-reading-tail value="${options.readingTailMs}" />
        </label>
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.maxCueDurationSec", "tools.audioSrt.hint.maxCueDurationSec")}
          <input type="number" min="1" max="10" step="1" data-opt-max-cue-sec value="${Math.round(options.maxCueDurationMs / 1000)}" />
        </label>
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.minCueDurationSec", "tools.audioSrt.hint.minCueDurationSec")}
          <input type="number" min="0.5" max="5" step="0.5" data-opt-min-cue-sec value="${options.minCueDurationMs / 1000}" />
        </label>
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.maxLineLength", "tools.audioSrt.hint.maxLineLength")}
          <input type="number" min="20" max="60" data-opt-max-line value="${options.maxLineLength}" />
        </label>
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.maxLines", "tools.audioSrt.hint.maxLines")}
          <select data-opt-max-lines>
            <option value="1"${options.maxLinesPerCue === 1 ? " selected" : ""}>1</option>
            <option value="2"${options.maxLinesPerCue === 2 ? " selected" : ""}>2</option>
          </select>
        </label>
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.globalOffsetMs", "tools.audioSrt.hint.globalOffsetMs")}
          <input type="number" data-opt-offset value="${options.globalOffsetMs}" />
        </label>
        <label class="field-compact field-compact--checkbox">
          <input type="checkbox" data-opt-bom ${options.utf8Bom ? "checked" : ""} />
          ${renderFieldTitle("tools.audioSrt.utf8Bom", "tools.audioSrt.hint.utf8Bom")}
        </label>
        <label class="field-compact field-compact--checkbox">
          <input type="checkbox" data-opt-dictation ${options.useDictationTextSettings ? "checked" : ""} />
          ${renderFieldTitle(
            "tools.audioSrt.useDictationTextSettings",
            "tools.audioSrt.hint.useDictationTextSettings",
          )}
        </label>
      </div>
    </details>
  `;
}

export function renderAudioSrtTool(
  jobs: AudioSrtJob[],
  selectedId: string | null,
  copyHint: string | null,
  saveHint: string | null,
  capability: SubtitleSttCapability,
  uiOptions: AudioSrtUiOptions,
): string {
  const selected =
    jobs.find((job) => job.id === selectedId) ??
    (jobs.length > 0 ? jobs[jobs.length - 1] : null);
  const resultText = selected?.status === "done" ? selected.srt : "";
  let displayText = resultText;
  if (selected?.status === "processing") {
    displayText = `${stageLabel(selected.processingPhase)} — ${selected.fileName}`;
  } else if (selected?.status === "pending") {
    displayText = `${t("tools.audioSrt.status.pending")} ${selected.fileName}`;
  } else if (selected?.status === "done" && !resultText.trim()) {
    displayText = t("tools.audioSrt.emptyResult");
  }
  const processing = jobs.some((job) => job.status === "processing");
  const progressPercent =
    selected?.status === "processing" && selected.processingPercent !== null
      ? selected.processingPercent
      : null;
  const canExport = Boolean(resultText.trim());
  const showPreview = selected?.status === "done" && Boolean(selected.path);
  const copyTitle = copyHint ?? t("tools.audioSrt.copy");
  const saveAsTitle = saveHint ?? t("tools.audioSrt.saveAs");
  const footnote = resultFootnote(selected);
  const footnoteClass = selected?.rewriteFallback
    ? "voice-files-fallback-hint"
    : "voice-files-stage-hint";
  const providerBlocked = capability === "unsupportedProvider";

  return `
    <main class="voice-files-tool audio-srt-tool">
      ${
        providerBlocked
          ? `<p class="voice-files-fallback-hint" role="status">${escapeHtml(t("tools.audioSrt.unsupportedProvider"))}</p>`
          : ""
      }
      <div
        class="voice-files-dropzone${processing || providerBlocked ? " voice-files-dropzone--busy" : ""}"
        data-audio-srt-dropzone
        tabindex="0"
        role="region"
        aria-label="${escapeHtml(t("tools.audioSrt.dropHint"))}"
      >
        <p class="voice-files-dropzone-text">${escapeHtml(t("tools.audioSrt.dropHint"))}</p>
        <button type="button" class="btn btn-secondary btn-compact" data-pick-audio-srt ${processing || providerBlocked ? "disabled" : ""}>
          ${escapeHtml(t("tools.audioSrt.pickFiles"))}
        </button>
        ${renderQueueCompact(jobs, selectedId)}
      </div>

      ${renderAdvancedOptions(uiOptions)}

      ${
        progressPercent !== null
          ? `
        <div
          class="voice-files-progress"
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${progressPercent}"
          aria-label="${escapeHtml(stageLabel(selected?.processingPhase ?? null))}"
        >
          <div class="voice-files-progress-fill" style="width: ${progressPercent}%"></div>
        </div>
      `
          : ""
      }

      <div class="audio-srt-actions">
        <button type="button" class="btn btn-secondary btn-compact" data-save-srt-as ${canExport ? "" : "disabled"}>
          ${escapeHtml(saveAsTitle)}
        </button>
      </div>

      <div class="audio-srt-preview-slot" data-audio-srt-preview-slot ${showPreview ? "" : "hidden"}></div>

      <div class="voice-files-text-stack" aria-live="polite">
        <div class="voice-files-text-wrap">
          <button
            type="button"
            class="voice-files-copy-overlay icon-btn"
            data-copy-srt-result
            ${canExport ? "" : "disabled"}
            aria-label="${escapeHtml(copyTitle)}"
            title="${escapeHtml(copyTitle)}"
          >${iconCopy()}</button>
          <textarea
            class="voice-files-textarea audio-srt-textarea"
            data-srt-result-text
            aria-label="${escapeHtml(t("tools.audioSrt.resultTitle"))}"
          >${escapeHtml(displayText)}</textarea>
        </div>
        ${footnote ? `<p class="${footnoteClass}">${escapeHtml(footnote)}</p>` : ""}
      </div>
    </main>
  `;
}

let jobCounter = 0;

function nextJobId(): string {
  jobCounter += 1;
  return `asrt-${jobCounter}`;
}

function buildApiOptions(ui: AudioSrtUiOptions): AudioSrtOptions {
  return {
    maxLineLength: ui.maxLineLength,
    maxLinesPerCue: ui.maxLinesPerCue,
    globalOffsetMs: ui.globalOffsetMs,
    utf8Bom: ui.utf8Bom,
    useDictationTextSettings: ui.useDictationTextSettings,
    smartSplit: ui.smartSplit,
    pauseSplitMs: ui.pauseSplitMs,
    maxCueDurationMs: ui.maxCueDurationMs,
    minCueDurationMs: ui.minCueDurationMs,
    readingTailMs: ui.readingTailMs,
  };
}

export function createAudioSrtController(root: HTMLElement): {
  enqueuePaths: (paths: string[]) => void;
  dispose: () => void;
} {
  let jobs: AudioSrtJob[] = [];
  let selectedId: string | null = null;
  let copyHint: string | null = null;
  let saveHint: string | null = null;
  let queueRunning = false;
  let progressUnlisten: UnlistenFn | null = null;
  let capability: SubtitleSttCapability = "supported";
  let uiOptions: AudioSrtUiOptions = { ...DEFAULT_UI_OPTIONS };
  const preview = createAudioSrtPreview();

  const syncPreview = (): void => {
    const slot = root.querySelector<HTMLElement>("[data-audio-srt-preview-slot]");
    if (!slot) {
      return;
    }
    mountAudioSrtPreview(slot, preview);
    const selected = selectedJob();
    const textarea = root.querySelector<HTMLTextAreaElement>("[data-srt-result-text]");
    const srtText =
      selected?.status === "done" ? (textarea?.value ?? selected.srt) : "";
    preview.update({
      audioPath: selected?.status === "done" ? selected.path : null,
      srtText,
      globalOffsetMs: uiOptions.globalOffsetMs,
      visible: Boolean(selected?.status === "done" && selected.path),
    });
  };

  void getSubtitleSttCapability()
    .then((value) => {
      capability = value;
      paint();
    })
    .catch(() => {
      capability = "supported";
    });

  const readUiOptionsFromDom = (): void => {
    const maxLine = root.querySelector<HTMLInputElement>("[data-opt-max-line]");
    const maxLines = root.querySelector<HTMLSelectElement>("[data-opt-max-lines]");
    const offset = root.querySelector<HTMLInputElement>("[data-opt-offset]");
    const bom = root.querySelector<HTMLInputElement>("[data-opt-bom]");
    const dictation = root.querySelector<HTMLInputElement>("[data-opt-dictation]");
    const smartSplit = root.querySelector<HTMLInputElement>("[data-opt-smart-split]");
    const pauseSplit = root.querySelector<HTMLInputElement>("[data-opt-pause-split]");
    const readingTail = root.querySelector<HTMLInputElement>("[data-opt-reading-tail]");
    const maxCueSec = root.querySelector<HTMLInputElement>("[data-opt-max-cue-sec]");
    const minCueSec = root.querySelector<HTMLInputElement>("[data-opt-min-cue-sec]");
    const advanced = root.querySelector<HTMLDetailsElement>("[data-audio-srt-advanced]");
    if (maxLine) {
      uiOptions.maxLineLength = Math.min(60, Math.max(20, Number(maxLine.value) || 42));
    }
    if (maxLines) {
      uiOptions.maxLinesPerCue = maxLines.value === "1" ? 1 : 2;
    }
    if (offset) {
      uiOptions.globalOffsetMs = Number(offset.value) || 0;
    }
    if (bom) {
      uiOptions.utf8Bom = bom.checked;
    }
    if (dictation) {
      uiOptions.useDictationTextSettings = dictation.checked;
    }
    if (smartSplit) {
      uiOptions.smartSplit = smartSplit.checked;
    }
    if (pauseSplit) {
      uiOptions.pauseSplitMs = Math.min(2000, Math.max(100, Number(pauseSplit.value) || 400));
    }
    if (readingTail) {
      uiOptions.readingTailMs = Math.min(1000, Math.max(0, Number(readingTail.value) || 200));
    }
    if (maxCueSec) {
      uiOptions.maxCueDurationMs =
        Math.min(10_000, Math.max(1000, Math.round(Number(maxCueSec.value) || 7) * 1000));
    }
    if (minCueSec) {
      uiOptions.minCueDurationMs =
        Math.min(5000, Math.max(500, Math.round((Number(minCueSec.value) || 1) * 1000)));
    }
    if (advanced) {
      uiOptions.advancedOpen = advanced.open;
    }
  };

  const syncEditedSrtFromTextarea = (): void => {
    const textarea = root.querySelector<HTMLTextAreaElement>("[data-srt-result-text]");
    if (!textarea || !selectedId) {
      return;
    }
    const selected = jobs.find((job) => job.id === selectedId);
    if (selected?.status === "done") {
      selected.srt = textarea.value;
    }
  };

  const onProgress = (payload: AudioSrtProgressPayload): void => {
    jobs = jobs.map((job) => {
      if (job.status !== "processing" || !pathsMatch(job.path, payload.path)) {
        return job;
      }
      const phaseChanged = payload.phase !== job.processingPhase;
      let processingPercent = job.processingPercent;
      if (payload.percent !== undefined) {
        processingPercent =
          typeof payload.percent === "number" && Number.isFinite(payload.percent)
            ? Math.min(100, Math.max(0, Math.round(payload.percent)))
            : null;
      } else if (phaseChanged) {
        processingPercent = null;
      }
      return {
        ...job,
        processingPhase: payload.phase,
        processingPercent,
      };
    });
    paint();
  };

  const paint = (): void => {
    root.innerHTML = renderAudioSrtTool(
      jobs,
      selectedId,
      copyHint,
      saveHint,
      capability,
      uiOptions,
    );
    bind();
    syncPreview();
  };

  const bind = (): void => {
    root.querySelector<HTMLButtonElement>("[data-pick-audio-srt]")?.addEventListener("click", () => {
      void pickAndEnqueue();
    });

    root.querySelector<HTMLButtonElement>("[data-copy-srt-result]")?.addEventListener("click", () => {
      void copyResult();
    });

    root.querySelector<HTMLButtonElement>("[data-save-srt-as]")?.addEventListener("click", () => {
      void saveResult();
    });

    root.querySelectorAll<HTMLButtonElement>("[data-audio-srt-id]").forEach((button) => {
      button.addEventListener("click", () => {
        syncEditedSrtFromTextarea();
        selectedId = button.dataset.audioSrtId ?? null;
        copyHint = null;
        saveHint = null;
        paint();
      });
    });

    root.querySelector<HTMLTextAreaElement>("[data-srt-result-text]")?.addEventListener("input", () => {
      syncEditedSrtFromTextarea();
      syncPreview();
    });

    root.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
      "[data-opt-max-line], [data-opt-max-lines], [data-opt-offset], [data-opt-bom], [data-opt-dictation], [data-opt-smart-split], [data-opt-pause-split], [data-opt-reading-tail], [data-opt-max-cue-sec], [data-opt-min-cue-sec]",
    ).forEach((element) => {
      element.addEventListener("change", () => {
        const smartBefore = uiOptions.smartSplit;
        readUiOptionsFromDom();
        if (element.matches("[data-opt-smart-split]") && smartBefore !== uiOptions.smartSplit) {
          paint();
        }
      });
    });

    root.querySelector<HTMLElement>(".audio-srt-advanced-body")?.addEventListener("click", (event) => {
      const target = event.target;
      if (!(target instanceof HTMLElement)) {
        return;
      }
      const button = target.closest<HTMLButtonElement>(".field-help-btn");
      if (!button) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const wrap = button.closest(".field-help-wrap");
      if (!wrap) {
        return;
      }
      const willOpen = !wrap.classList.contains("field-help-wrap--open");
      root.querySelectorAll(".field-help-wrap--open").forEach((openWrap) => {
        openWrap.classList.remove("field-help-wrap--open");
        openWrap.querySelector(".field-help-btn")?.setAttribute("aria-expanded", "false");
      });
      if (willOpen) {
        wrap.classList.add("field-help-wrap--open");
        button.setAttribute("aria-expanded", "true");
      }
    });

    root.querySelector<HTMLDetailsElement>("[data-audio-srt-advanced]")?.addEventListener("toggle", () => {
      readUiOptionsFromDom();
    });

    const dropzone = root.querySelector<HTMLElement>("[data-audio-srt-dropzone]");
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
      if (!transfer?.files?.length) {
        return;
      }
      const paths: string[] = [];
      for (const file of Array.from(transfer.files)) {
        const withPath = file as File & { path?: string };
        if (withPath.path) {
          paths.push(withPath.path);
        }
      }
      if (paths.length > 0) {
        enqueuePaths(paths);
      }
    });
  };

  const pickAndEnqueue = async (): Promise<void> => {
    const paths = await pickVoiceFiles();
    if (paths.length > 0) {
      enqueuePaths(paths);
    }
  };

  const selectedJob = (): AudioSrtJob | undefined => {
    return (
      jobs.find((job) => job.id === selectedId) ??
      (jobs.length > 0 ? jobs[jobs.length - 1] : undefined)
    );
  };

  const copyResult = async (): Promise<void> => {
    syncEditedSrtFromTextarea();
    const selected = selectedJob();
    if (!selected?.srt.trim()) {
      return;
    }
    try {
      await copyTextToClipboard(selected.srt);
      copyHint = t("tools.audioSrt.copied");
    } catch {
      try {
        await navigator.clipboard.writeText(selected.srt);
        copyHint = t("tools.audioSrt.copied");
      } catch {
        copyHint = t("tools.audioSrt.copyFailed");
      }
    }
    paint();
    window.setTimeout(() => {
      copyHint = null;
      paint();
    }, 1600);
  };

  const defaultSrtName = (fileName: string): string => {
    const base = fileName.replace(/\.[^.\\/]+$/, "");
    return `${base || "subtitles"}.srt`;
  };

  const saveResult = async (): Promise<void> => {
    syncEditedSrtFromTextarea();
    const selected = selectedJob();
    if (!selected?.srt.trim()) {
      return;
    }
    readUiOptionsFromDom();
    try {
      const targetPath = await pickSubtitleSavePath(defaultSrtName(selected.fileName));
      if (!targetPath) {
        return;
      }
      await saveSubtitleFile(targetPath, selected.srt, uiOptions.utf8Bom);
      saveHint = t("tools.audioSrt.saved");
    } catch {
      saveHint = t("tools.audioSrt.saveFailed");
    }
    paint();
    window.setTimeout(() => {
      saveHint = null;
      paint();
    }, 1600);
  };

  const enqueuePaths = (paths: string[]): void => {
    if (capability === "unsupportedProvider") {
      return;
    }
    readUiOptionsFromDom();
    const unique = paths.filter((path) => path.trim().length > 0);
    if (unique.length === 0) {
      return;
    }
    for (const path of unique) {
      const id = nextJobId();
      jobs.push({
        id,
        path,
        fileName: fileNameFromPath(path),
        status: "pending",
        processingPhase: null,
        processingPercent: null,
        srt: "",
        errorKey: null,
        rewriteFallback: false,
        aiRewriteApplied: false,
      });
      if (!selectedId) {
        selectedId = id;
      }
    }
    paint();
    void runQueue();
  };

  const applyResult = (id: string, result: AudioSrtTranscriptionResult): void => {
    jobs = jobs.map((job) =>
      job.id === id
        ? {
            ...job,
            status: "done",
            srt: result.srt,
            rewriteFallback: result.rewriteFallback,
            aiRewriteApplied: result.aiRewriteApplied,
            processingPhase: null,
            processingPercent: null,
            fileName: result.fileName || job.fileName,
          }
        : job,
    );
    selectedId = id;
  };

  const applyError = (id: string, errorKey: string): void => {
    jobs = jobs.map((job) =>
      job.id === id
        ? {
            ...job,
            status: "error",
            errorKey,
          }
        : job,
    );
    selectedId = id;
  };

  const runQueue = async (): Promise<void> => {
    if (queueRunning) {
      return;
    }
    queueRunning = true;
    try {
      while (true) {
        const next = jobs.find((job) => job.status === "pending");
        if (!next) {
          break;
        }
        jobs = jobs.map((job) =>
          job.id === next.id
            ? {
                ...job,
                status: "processing",
                processingPhase: "decoding",
                processingPercent: 0,
              }
            : job,
        );
        selectedId = next.id;
        copyHint = null;
        saveHint = null;
        paint();

        try {
          const result = await runToolWithSttLanguageRecovery(() =>
            transcribeAudioToSrt(next.path, buildApiOptions(uiOptions)),
          );
          applyResult(next.id, result);
        } catch (error) {
          const raw = error instanceof Error ? error.message : String(error);
          const key = raw.startsWith("tools.") ? raw : "tools.audioSrt.failed";
          applyError(next.id, key);
        }
        paint();
      }
    } finally {
      queueRunning = false;
    }
  };

  void listen<AudioSrtProgressPayload>(EVENTS.audioSrtProgress, (event) => {
    onProgress(event.payload);
  }).then((unlisten) => {
    progressUnlisten = unlisten;
  });

  paint();

  return {
    enqueuePaths,
    dispose: (): void => {
      preview.destroy();
      void progressUnlisten?.();
      progressUnlisten = null;
    },
  };
}
