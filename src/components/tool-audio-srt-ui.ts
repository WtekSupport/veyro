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
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import {
  formatToolErrorForResultField,
  toolErrorMessageKey,
} from "../lib/tool-error-display";
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
  errorRaw: string | null;
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

function statusLabel(job: AudioSrtJob, visiblePercent: number | null): string {
  switch (job.status) {
    case "pending":
      return t("tools.audioSrt.status.pending");
    case "processing": {
      const stage = stageLabel(job.processingPhase);
      const percent =
        visiblePercent !== null && job.processingPhase === "decoding"
          ? visiblePercent
          : job.processingPercent;
      if (percent !== null && percent >= 0) {
        return `${stage} ${percent}%`;
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

function renderQueueCompact(
  jobs: AudioSrtJob[],
  selectedId: string | null,
  visiblePercent: number | null,
): string {
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
                <span class="voice-files-queue-chip-status">${escapeHtml(statusLabel(job, visiblePercent))}</span>
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

/** Matches clamps in `src-tauri/src/tools/audio_to_srt.rs`. */
const SRT_ADVANCED_LIMITS = {
  pauseSplitMs: { min: 100, max: 2000, step: 50 },
  readingTailMs: { min: 0, max: 1000, step: 50 },
  maxCueDurationMs: { min: 1000, max: 10_000, step: 500 },
  minCueDurationMs: { min: 500, max: 5000, step: 250 },
  maxLineLength: { min: 20, max: 60, step: 1 },
  globalOffsetMs: { min: -3000, max: 3000, step: 50 },
} as const;

type SrtSliderFormat = "ms" | "sec-dec" | "sec-int" | "chars" | "offset";

function clampSrtSliderValue(
  key: keyof typeof SRT_ADVANCED_LIMITS,
  value: number,
): number {
  const { min, max, step } = SRT_ADVANCED_LIMITS[key];
  const snapped = Math.round((value - min) / step) * step + min;
  return Math.min(max, Math.max(min, snapped));
}

function formatSrtSliderDisplay(value: number, format: SrtSliderFormat): string {
  const rounded = Math.round(value);
  switch (format) {
    case "ms":
      return `${rounded} ms`;
    case "sec-dec":
      return `${(rounded / 1000).toFixed(1)} s`;
    case "sec-int":
      return `${Math.round(rounded / 1000)} s`;
    case "chars":
      return String(rounded);
    case "offset":
      return rounded > 0 ? `+${rounded} ms` : `${rounded} ms`;
  }
}

function renderAdvancedSlider(
  sliderId: string,
  labelKey: MessageKey,
  hintKey: MessageKey,
  dataOptAttr: string,
  value: number,
  limits: { min: number; max: number; step: number },
  format: SrtSliderFormat,
): string {
  const snapped = Math.min(limits.max, Math.max(limits.min, value));
  const display = formatSrtSliderDisplay(snapped, format);
  const minLabel = formatSrtSliderDisplay(limits.min, format);
  const maxLabel = formatSrtSliderDisplay(limits.max, format);
  return `
    <label class="field-compact audio-srt-slider-field">
      <div class="audio-srt-slider-header">
        <span class="audio-srt-slider-title">${renderFieldTitle(labelKey, hintKey)}</span>
        <span class="audio-srt-slider-value" data-srt-slider-value-for="${escapeHtml(sliderId)}">${escapeHtml(display)}</span>
      </div>
      <input
        type="range"
        ${dataOptAttr}
        data-srt-slider-id="${escapeHtml(sliderId)}"
        data-srt-slider-format="${format}"
        min="${limits.min}"
        max="${limits.max}"
        step="${limits.step}"
        value="${snapped}"
      />
      <div class="audio-srt-slider-bounds" aria-hidden="true">
        <span>${escapeHtml(minLabel)}</span>
        <span>${escapeHtml(maxLabel)}</span>
      </div>
    </label>
  `;
}

function syncSrtSliderLabels(root: HTMLElement): void {
  root.querySelectorAll<HTMLInputElement>("input[data-srt-slider-format]").forEach((input) => {
    const id = input.dataset.srtSliderId;
    if (!id) {
      return;
    }
    const span = root.querySelector<HTMLElement>(`[data-srt-slider-value-for="${id}"]`);
    const format = input.dataset.srtSliderFormat as SrtSliderFormat | undefined;
    if (!span || !format) {
      return;
    }
    span.textContent = formatSrtSliderDisplay(Number(input.value), format);
  });
}

function renderAdvancedOptions(options: AudioSrtUiOptions): string {
  const pauseSplitField = options.smartSplit
    ? ""
    : renderAdvancedSlider(
        "pauseSplit",
        "tools.audioSrt.pauseSplitMs",
        "tools.audioSrt.hint.pauseSplitMs",
        "data-opt-pause-split",
        options.pauseSplitMs,
        SRT_ADVANCED_LIMITS.pauseSplitMs,
        "ms",
      );

  return `
    <details class="audio-srt-advanced"${options.advancedOpen ? " open" : ""} data-audio-srt-advanced>
      <summary>${escapeHtml(t("tools.audioSrt.advanced"))}</summary>
      <div class="audio-srt-advanced-body">
        <label class="field-compact field-compact--checkbox">
          <input type="checkbox" data-opt-smart-split ${options.smartSplit ? "checked" : ""} />
          ${renderFieldTitle("tools.audioSrt.smartSplit", "tools.audioSrt.hint.smartSplit")}
        </label>
        ${pauseSplitField}
        ${renderAdvancedSlider(
          "readingTail",
          "tools.audioSrt.readingTailMs",
          "tools.audioSrt.hint.readingTailMs",
          "data-opt-reading-tail",
          options.readingTailMs,
          SRT_ADVANCED_LIMITS.readingTailMs,
          "ms",
        )}
        ${renderAdvancedSlider(
          "maxCueDuration",
          "tools.audioSrt.maxCueDurationSec",
          "tools.audioSrt.hint.maxCueDurationSec",
          "data-opt-max-cue-sec",
          options.maxCueDurationMs,
          SRT_ADVANCED_LIMITS.maxCueDurationMs,
          "sec-int",
        )}
        ${renderAdvancedSlider(
          "minCueDuration",
          "tools.audioSrt.minCueDurationSec",
          "tools.audioSrt.hint.minCueDurationSec",
          "data-opt-min-cue-sec",
          options.minCueDurationMs,
          SRT_ADVANCED_LIMITS.minCueDurationMs,
          "sec-dec",
        )}
        ${renderAdvancedSlider(
          "maxLineLength",
          "tools.audioSrt.maxLineLength",
          "tools.audioSrt.hint.maxLineLength",
          "data-opt-max-line",
          options.maxLineLength,
          SRT_ADVANCED_LIMITS.maxLineLength,
          "chars",
        )}
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.maxLines", "tools.audioSrt.hint.maxLines")}
          <select data-opt-max-lines>
            <option value="1"${options.maxLinesPerCue === 1 ? " selected" : ""}>1</option>
            <option value="2"${options.maxLinesPerCue === 2 ? " selected" : ""}>2</option>
          </select>
        </label>
        ${renderAdvancedSlider(
          "globalOffset",
          "tools.audioSrt.globalOffsetMs",
          "tools.audioSrt.hint.globalOffsetMs",
          "data-opt-offset",
          options.globalOffsetMs,
          SRT_ADVANCED_LIMITS.globalOffsetMs,
          "offset",
        )}
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
  visibleProgressPercent: number | null = null,
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
  } else if (selected?.status === "error" && selected.errorRaw) {
    displayText = formatToolErrorForResultField(
      selected.errorRaw,
      "tools.audioSrt.failed",
    );
  } else if (selected?.status === "error") {
    displayText = formatToolErrorForResultField(
      selected.errorKey ?? "tools.audioSrt.failed",
      "tools.audioSrt.failed",
    );
  }
  const processing = jobs.some((job) => job.status === "processing");
  const progressPercent =
    selected?.status === "processing"
      ? (visibleProgressPercent ?? selected.processingPercent)
      : null;
  const canExport = Boolean(resultText.trim());
  const canCopy = Boolean(displayText.trim()) && selected?.status !== "pending";
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
        ${renderQueueCompact(jobs, selectedId, visibleProgressPercent)}
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
            ${canCopy ? "" : "disabled"}
            aria-label="${escapeHtml(copyTitle)}"
            title="${escapeHtml(copyTitle)}"
          >${iconCopy()}</button>
          <textarea
            class="voice-files-textarea audio-srt-textarea${selected?.status === "error" ? " voice-files-textarea--error" : ""}"
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
  const decodeProgress = new ToolDecodeProgressSmoother();

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
      uiOptions.maxLineLength = clampSrtSliderValue(
        "maxLineLength",
        Number(maxLine.value) || 42,
      );
    }
    if (maxLines) {
      uiOptions.maxLinesPerCue = maxLines.value === "1" ? 1 : 2;
    }
    if (offset) {
      uiOptions.globalOffsetMs = clampSrtSliderValue(
        "globalOffsetMs",
        Number(offset.value) || 0,
      );
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
      uiOptions.pauseSplitMs = clampSrtSliderValue(
        "pauseSplitMs",
        Number(pauseSplit.value) || 400,
      );
    }
    if (readingTail) {
      uiOptions.readingTailMs = clampSrtSliderValue(
        "readingTailMs",
        Number(readingTail.value) || 200,
      );
    }
    if (maxCueSec) {
      uiOptions.maxCueDurationMs = clampSrtSliderValue(
        "maxCueDurationMs",
        Number(maxCueSec.value) || 7000,
      );
    }
    if (minCueSec) {
      uiOptions.minCueDurationMs = clampSrtSliderValue(
        "minCueDurationMs",
        Number(minCueSec.value) || 1000,
      );
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
        processingPercent = 0;
      }
      return {
        ...job,
        processingPhase: payload.phase,
        processingPercent,
      };
    });
    const active = jobs.find(
      (job) => job.status === "processing" && pathsMatch(job.path, payload.path),
    );
    decodeProgress.sync(
      active?.processingPhase ?? null,
      active?.processingPercent ?? null,
    );
    paint();
  };

  const paint = (): void => {
    const selected =
      jobs.find((job) => job.id === selectedId) ??
      (jobs.length > 0 ? jobs[jobs.length - 1] : undefined);
    const visible = decodeProgress.displayPercent(
      selected?.processingPhase ?? null,
      selected?.processingPercent ?? null,
    );
    root.innerHTML = renderAudioSrtTool(
      jobs,
      selectedId,
      copyHint,
      saveHint,
      capability,
      uiOptions,
      visible,
    );
    bind();
    syncPreview();
  };

  decodeProgress.bindRepaint(paint);

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
      const onUpdate = (): void => {
        const smartBefore = uiOptions.smartSplit;
        readUiOptionsFromDom();
        syncSrtSliderLabels(root);
        if (element.matches("[data-opt-smart-split]") && smartBefore !== uiOptions.smartSplit) {
          paint();
        }
      };
      element.addEventListener("change", onUpdate);
      if (element instanceof HTMLInputElement && element.type === "range") {
        element.addEventListener("input", onUpdate);
      }
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
    const textarea = root.querySelector<HTMLTextAreaElement>("[data-srt-result-text]");
    const fromDom = textarea?.value.trim() ?? "";
    const selected = selectedJob();
    const payload =
      fromDom ||
      (selected?.status === "done" ? selected.srt.trim() : "") ||
      (selected?.status === "error" && selected.errorRaw
        ? formatToolErrorForResultField(selected.errorRaw, "tools.audioSrt.failed")
        : "");
    if (!payload) {
      return;
    }
    try {
      await copyTextToClipboard(payload);
      copyHint = t("tools.audioSrt.copied");
    } catch {
      try {
        await navigator.clipboard.writeText(payload);
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
        errorRaw: null,
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

  const applyError = (id: string, errorRaw: string): void => {
    const errorKey = toolErrorMessageKey(errorRaw, "tools.audioSrt.failed");
    jobs = jobs.map((job) =>
      job.id === id
        ? {
            ...job,
            status: "error",
            errorKey,
            errorRaw,
            processingPhase: null,
            processingPercent: null,
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
        decodeProgress.reset();
        paint();

        try {
          const result = await runToolWithSttLanguageRecovery(() =>
            transcribeAudioToSrt(next.path, buildApiOptions(uiOptions)),
          );
          applyResult(next.id, result);
        } catch (error) {
          const raw = error instanceof Error ? error.message : String(error);
          applyError(next.id, raw.trim() || "tools.audioSrt.failed");
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
      decodeProgress.dispose();
    },
  };
}
