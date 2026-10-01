import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  downloadDiarizationModel,
  EVENTS,
  getDiarizationModelStatus,
  getSubtitleSttCapability,
  pickSubtitleSavePath,
  pickVoiceFiles,
  saveSubtitleFile,
  transcribeAudioToSrt,
  type AudioSrtOptions,
  type AudioSrtProgressPayload,
  type AudioSrtProgressPhase,
  type AudioSrtSpeakerCountMode,
  type AudioSrtSpeakerInfo,
  type AudioSrtTranscriptionResult,
  type SubtitleSttCapability,
} from "../api";
import {
  createAudioSrtPreview,
  mountAudioSrtPreview,
  speakerColorIndex,
} from "./audio-srt-preview";
import { showConfirmDialog } from "./confirm-dialog";
import { iconCopy } from "./icons";
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import {
  formatToolErrorForResultField,
  toolErrorMessageKey,
} from "../lib/tool-error-display";
import { runToolWithSttLanguageRecovery } from "../lib/tool-stt-auto-recovery";
import { t, type MessageKey } from "../i18n";
import { escapeHtml } from "../lib/html";

export type AudioSrtJobStatus = "pending" | "processing" | "done" | "error";

export interface AudioSrtJob {
  id: string;
  path: string;
  fileName: string;
  status: AudioSrtJobStatus;
  processingPhase: AudioSrtProgressPhase | null;
  processingPercent: number | null;
  srt: string;
  /** WebVTT companion export (`<v Name>` when speakers exist). */
  vtt: string;
  errorKey: string | null;
  errorRaw: string | null;
  rewriteFallback: boolean;
  aiRewriteApplied: boolean;
  speakers: AudioSrtSpeakerInfo[];
  cueSpeakerIds: Array<number | null>;
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
  speakerDiarization: boolean;
  speakerCountMode: AudioSrtSpeakerCountMode;
  speakerExactCount: number;
  speakerMinCount: number;
  speakerMaxCount: number;
  includeSpeakerNames: boolean;
  diarizationMinSpeechSecs: number;
  diarizationSensitivity: number;
  diarizationExpertOpen: boolean;
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
  speakerDiarization: false,
  speakerCountMode: "auto",
  speakerExactCount: 2,
  speakerMinCount: 1,
  speakerMaxCount: 8,
  includeSpeakerNames: true,
  diarizationMinSpeechSecs: 0.25,
  diarizationSensitivity: 0.45,
  diarizationExpertOpen: false,
};

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function fileNameFromPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const segment = normalized.split("/").pop();
  return segment ?? path;
}

function pathsMatch(left: string, right: string): boolean {
  return left.replace(/\\/g, "/").toLowerCase() === right.replace(/\\/g, "/").toLowerCase();
}

/** Replace speaker prefixes in cue text (`Label: …`). */
export function rewriteSrtSpeakerLabels(
  srt: string,
  fromLabel: string,
  toLabel: string,
): string {
  const from = fromLabel.trim();
  const to = toLabel.trim();
  if (!from || !to || from === to) {
    return srt;
  }
  const pattern = new RegExp(`(^|\\n)${escapeRegExp(from)}: `, "g");
  return srt.replace(pattern, `$1${to}: `);
}

/** Replace WebVTT voice tags `<v OldName>` → `<v NewName>`. */
export function rewriteVttVoiceTags(
  vtt: string,
  fromLabel: string,
  toLabel: string,
): string {
  const from = fromLabel.trim();
  const to = toLabel.trim();
  if (!from || !to || from === to) {
    return vtt;
  }
  const pattern = new RegExp(`<v ${escapeRegExp(from)}>`, "g");
  return vtt.replace(pattern, `<v ${to}>`);
}

/** Merge `fromId` into `intoId`: update cue ids, drop from speaker list, rewrite prefixes. */
export function mergeSpeakers(
  srt: string,
  vtt: string,
  speakers: AudioSrtSpeakerInfo[],
  cueSpeakerIds: Array<number | null>,
  fromId: number,
  intoId: number,
): {
  srt: string;
  vtt: string;
  speakers: AudioSrtSpeakerInfo[];
  cueSpeakerIds: Array<number | null>;
} {
  if (fromId === intoId) {
    return { srt, vtt, speakers, cueSpeakerIds };
  }
  const from = speakers.find((speaker) => speaker.id === fromId);
  const into = speakers.find((speaker) => speaker.id === intoId);
  if (!from || !into) {
    return { srt, vtt, speakers, cueSpeakerIds };
  }
  return {
    srt: rewriteSrtSpeakerLabels(srt, from.label, into.label),
    vtt: rewriteVttVoiceTags(vtt, from.label, into.label),
    speakers: speakers.filter((speaker) => speaker.id !== fromId),
    cueSpeakerIds: cueSpeakerIds.map((id) => (id === fromId ? intoId : id)),
  };
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
    case "diarizing":
      return t("tools.audioSrt.stage.diarizing");
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

const DIARIZATION_LIMITS = {
  speakerCount: { min: 1, max: 20 },
  minSpeechSecs: { min: 0.05, max: 2, step: 0.05 },
  sensitivity: { min: 0.15, max: 0.9, step: 0.05 },
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

function clampSpeakerCount(value: number): number {
  const { min, max } = DIARIZATION_LIMITS.speakerCount;
  return Math.min(max, Math.max(min, Math.round(value) || min));
}

function clampDiarizationFloat(
  value: number,
  limits: { min: number; max: number; step: number },
  fallback: number,
): number {
  const raw = Number.isFinite(value) ? value : fallback;
  const snapped =
    Math.round((raw - limits.min) / limits.step) * limits.step + limits.min;
  return Math.min(limits.max, Math.max(limits.min, Number(snapped.toFixed(2))));
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
    <div class="field-compact audio-srt-slider-field">
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
        aria-label="${escapeHtml(t(labelKey))}"
      />
      <div class="audio-srt-slider-bounds" aria-hidden="true">
        <span>${escapeHtml(minLabel)}</span>
        <span>${escapeHtml(maxLabel)}</span>
      </div>
    </div>
  `;
}

function renderFloatSlider(
  sliderId: string,
  labelKey: MessageKey,
  hintKey: MessageKey,
  dataOptAttr: string,
  value: number,
  limits: { min: number; max: number; step: number },
  unit: string,
): string {
  const snapped = clampDiarizationFloat(value, limits, value);
  const display = `${snapped.toFixed(2)} ${unit}`;
  return `
    <div class="field-compact audio-srt-slider-field">
      <div class="audio-srt-slider-header">
        <span class="audio-srt-slider-title">${renderFieldTitle(labelKey, hintKey)}</span>
        <span class="audio-srt-slider-value" data-srt-slider-value-for="${escapeHtml(sliderId)}">${escapeHtml(display)}</span>
      </div>
      <input
        type="range"
        ${dataOptAttr}
        data-srt-slider-id="${escapeHtml(sliderId)}"
        data-srt-float-slider="1"
        data-srt-float-unit="${escapeHtml(unit)}"
        min="${limits.min}"
        max="${limits.max}"
        step="${limits.step}"
        value="${snapped}"
        aria-label="${escapeHtml(t(labelKey))}"
      />
      <div class="audio-srt-slider-bounds" aria-hidden="true">
        <span>${escapeHtml(`${limits.min} ${unit}`)}</span>
        <span>${escapeHtml(`${limits.max} ${unit}`)}</span>
      </div>
    </div>
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
  root.querySelectorAll<HTMLInputElement>("input[data-srt-float-slider]").forEach((input) => {
    const id = input.dataset.srtSliderId;
    if (!id) {
      return;
    }
    const span = root.querySelector<HTMLElement>(`[data-srt-slider-value-for="${id}"]`);
    if (!span) {
      return;
    }
    const unit = input.dataset.srtFloatUnit ?? "";
    const value = Number(input.value);
    span.textContent = `${value.toFixed(2)}${unit ? ` ${unit}` : ""}`;
  });
}

function renderDiarizationOptions(
  options: AudioSrtUiOptions,
  diarizationAvailable: boolean | null,
): string {
  const unavailable = diarizationAvailable === false;
  const enabled = options.speakerDiarization && !unavailable;
  const countControls =
    options.speakerCountMode === "exact"
      ? `
        <label class="field-compact">
          <span class="field-compact-title">${escapeHtml(t("tools.audioSrt.speakerExactCount"))}</span>
          <input type="number" data-opt-speaker-exact min="1" max="20" step="1" value="${options.speakerExactCount}" />
        </label>
      `
      : options.speakerCountMode === "range"
        ? `
        <div class="audio-srt-speaker-range">
          <label class="field-compact">
            <span class="field-compact-title">${escapeHtml(t("tools.audioSrt.speakerMinCount"))}</span>
            <input type="number" data-opt-speaker-min min="1" max="20" step="1" value="${options.speakerMinCount}" />
          </label>
          <label class="field-compact">
            <span class="field-compact-title">${escapeHtml(t("tools.audioSrt.speakerMaxCount"))}</span>
            <input type="number" data-opt-speaker-max min="1" max="20" step="1" value="${options.speakerMaxCount}" />
          </label>
        </div>
      `
        : "";

  const body = enabled
    ? `
      <div class="audio-srt-diarization-body">
        <label class="field-compact">
          ${renderFieldTitle("tools.audioSrt.speakerCountMode", "tools.audioSrt.hint.speakerCountMode")}
          <select data-opt-speaker-count-mode>
            <option value="auto"${options.speakerCountMode === "auto" ? " selected" : ""}>${escapeHtml(t("tools.audioSrt.speakerCountAuto"))}</option>
            <option value="exact"${options.speakerCountMode === "exact" ? " selected" : ""}>${escapeHtml(t("tools.audioSrt.speakerCountExact"))}</option>
            <option value="range"${options.speakerCountMode === "range" ? " selected" : ""}>${escapeHtml(t("tools.audioSrt.speakerCountRange"))}</option>
          </select>
        </label>
        ${countControls}
        <label class="field-compact field-compact--checkbox">
          <input type="checkbox" data-opt-include-speaker-names ${options.includeSpeakerNames ? "checked" : ""} />
          ${renderFieldTitle("tools.audioSrt.includeSpeakerNames", "tools.audioSrt.hint.includeSpeakerNames")}
        </label>
        <details class="audio-srt-diarization-expert"${options.diarizationExpertOpen ? " open" : ""} data-audio-srt-diarization-expert>
          <summary>${escapeHtml(t("tools.audioSrt.diarizationExpert"))}</summary>
          <div class="audio-srt-diarization-expert-body">
            ${renderFloatSlider(
              "diarMinSpeech",
              "tools.audioSrt.diarizationMinSpeechSecs",
              "tools.audioSrt.hint.diarizationMinSpeechSecs",
              "data-opt-diar-min-speech",
              options.diarizationMinSpeechSecs,
              DIARIZATION_LIMITS.minSpeechSecs,
              "s",
            )}
            ${renderFloatSlider(
              "diarSensitivity",
              "tools.audioSrt.diarizationSensitivity",
              "tools.audioSrt.hint.diarizationSensitivity",
              "data-opt-diar-sensitivity",
              options.diarizationSensitivity,
              DIARIZATION_LIMITS.sensitivity,
              "",
            )}
          </div>
        </details>
      </div>
    `
    : "";

  return `
    <section class="audio-srt-diarization" data-audio-srt-diarization>
      <label class="field-compact field-compact--checkbox${unavailable ? " field-compact--disabled" : ""}">
        <input
          type="checkbox"
          data-opt-speaker-diarization
          ${enabled ? "checked" : ""}
          ${unavailable ? "disabled" : ""}
        />
        ${renderFieldTitle("tools.audioSrt.speakerDiarization", "tools.audioSrt.hint.speakerDiarization")}
      </label>
      ${
        unavailable
          ? `<p class="voice-files-stage-hint" role="status">${escapeHtml(t("tools.audioSrt.diarizationUnavailable"))}</p>`
          : ""
      }
      ${body}
    </section>
  `;
}

function renderSpeakersPanel(job: AudioSrtJob | null | undefined): string {
  if (!job || job.status !== "done" || job.speakers.length === 0) {
    return "";
  }
  const rows = job.speakers
    .map((speaker) => {
      const colorIndex = speakerColorIndex(speaker.id) ?? 0;
      const mergeOptions = job.speakers
        .filter((other) => other.id !== speaker.id)
        .map(
          (other) =>
            `<option value="${other.id}">${escapeHtml(other.label)}</option>`,
        )
        .join("");
      return `
        <div class="audio-srt-speaker-row" data-speaker-row="${speaker.id}">
          <span
            class="audio-srt-speaker-swatch"
            style="--badge-speaker: var(--audio-srt-speaker-${colorIndex})"
            aria-hidden="true"
          ></span>
          <input
            type="text"
            class="audio-srt-speaker-label"
            data-speaker-label="${speaker.id}"
            value="${escapeHtml(speaker.label)}"
            aria-label="${escapeHtml(t("tools.audioSrt.renameSpeaker"))}"
          />
          <select
            class="audio-srt-speaker-merge"
            data-speaker-merge-from="${speaker.id}"
            aria-label="${escapeHtml(t("tools.audioSrt.mergeInto"))}"
            ${mergeOptions ? "" : "disabled"}
          >
            <option value="">${escapeHtml(t("tools.audioSrt.mergeInto"))}</option>
            ${mergeOptions}
          </select>
        </div>
      `;
    })
    .join("");

  return `
    <section class="audio-srt-speakers" data-audio-srt-speakers aria-label="${escapeHtml(t("tools.audioSrt.speakersTitle"))}">
      <h3 class="audio-srt-speakers-title">${escapeHtml(t("tools.audioSrt.speakersTitle"))}</h3>
      <div class="audio-srt-speakers-list">${rows}</div>
    </section>
  `;
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
  statusHint: string | null = null,
  diarizationAvailable: boolean | null = null,
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
  const saveSrtTitle = t("tools.audioSrt.save");
  const saveVttTitle = t("tools.audioSrt.saveVtt");
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

      ${renderDiarizationOptions(uiOptions, diarizationAvailable)}
      ${renderAdvancedOptions(uiOptions)}

      ${
        statusHint
          ? `<p class="voice-files-stage-hint" role="status">${escapeHtml(statusHint)}</p>`
          : ""
      }

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
        <button type="button" class="btn btn-secondary btn-compact" data-save-srt ${canExport ? "" : "disabled"}>
          ${escapeHtml(saveSrtTitle)}
        </button>
        <button type="button" class="btn btn-secondary btn-compact" data-save-vtt ${canExport ? "" : "disabled"}>
          ${escapeHtml(saveVttTitle)}
        </button>
        <button type="button" class="btn btn-secondary btn-compact" data-save-srt-as ${canExport ? "" : "disabled"}>
          ${escapeHtml(saveAsTitle)}
        </button>
      </div>

      <div class="audio-srt-preview-slot" data-audio-srt-preview-slot ${showPreview ? "" : "hidden"}></div>

      ${renderSpeakersPanel(selected)}

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
  const base: AudioSrtOptions = {
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
    speakerDiarization: ui.speakerDiarization,
  };
  if (!ui.speakerDiarization) {
    return base;
  }
  return {
    ...base,
    speakerCountMode: ui.speakerCountMode,
    speakerExactCount: ui.speakerExactCount,
    speakerMinCount: ui.speakerMinCount,
    speakerMaxCount: ui.speakerMaxCount,
    includeSpeakerNames: ui.includeSpeakerNames,
    diarizationMinSpeechSecs: ui.diarizationMinSpeechSecs,
    diarizationSensitivity: ui.diarizationSensitivity,
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
  let statusHint: string | null = null;
  let queueRunning = false;
  let progressUnlisten: UnlistenFn | null = null;
  let capability: SubtitleSttCapability = "supported";
  /** null = still probing backend feature flag */
  let diarizationAvailable: boolean | null = null;
  let uiOptions: AudioSrtUiOptions = { ...DEFAULT_UI_OPTIONS };
  const preview = createAudioSrtPreview();
  const decodeProgress = new ToolDecodeProgressSmoother();

  const selectedJob = (): AudioSrtJob | undefined => {
    return (
      jobs.find((job) => job.id === selectedId) ??
      (jobs.length > 0 ? jobs[jobs.length - 1] : undefined)
    );
  };

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
      speakers: selected?.status === "done" ? selected.speakers : [],
      cueSpeakerIds: selected?.status === "done" ? selected.cueSpeakerIds : [],
      onSpeakerBadgeClick: (speakerId) => {
        const input = root.querySelector<HTMLInputElement>(
          `[data-speaker-label="${speakerId}"]`,
        );
        input?.focus();
        input?.select();
      },
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

  void getDiarizationModelStatus()
    .then((status) => {
      diarizationAvailable = status.available;
      if (!status.available) {
        uiOptions.speakerDiarization = false;
        if (statusHint === t("tools.audioSrt.diarizationUnavailable")) {
          statusHint = null;
        }
      }
      paint();
    })
    .catch(() => {
      diarizationAvailable = false;
      uiOptions.speakerDiarization = false;
      paint();
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
    const speakerDiarization = root.querySelector<HTMLInputElement>(
      "[data-opt-speaker-diarization]",
    );
    const speakerCountMode = root.querySelector<HTMLSelectElement>(
      "[data-opt-speaker-count-mode]",
    );
    const speakerExact = root.querySelector<HTMLInputElement>("[data-opt-speaker-exact]");
    const speakerMin = root.querySelector<HTMLInputElement>("[data-opt-speaker-min]");
    const speakerMax = root.querySelector<HTMLInputElement>("[data-opt-speaker-max]");
    const includeNames = root.querySelector<HTMLInputElement>(
      "[data-opt-include-speaker-names]",
    );
    const diarMinSpeech = root.querySelector<HTMLInputElement>("[data-opt-diar-min-speech]");
    const diarSensitivity = root.querySelector<HTMLInputElement>(
      "[data-opt-diar-sensitivity]",
    );
    const diarExpert = root.querySelector<HTMLDetailsElement>(
      "[data-audio-srt-diarization-expert]",
    );
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
    if (speakerDiarization) {
      uiOptions.speakerDiarization = speakerDiarization.checked;
    }
    if (speakerCountMode) {
      const mode = speakerCountMode.value;
      uiOptions.speakerCountMode =
        mode === "exact" || mode === "range" ? mode : "auto";
    }
    if (speakerExact) {
      uiOptions.speakerExactCount = clampSpeakerCount(Number(speakerExact.value));
    }
    if (speakerMin) {
      uiOptions.speakerMinCount = clampSpeakerCount(Number(speakerMin.value));
    }
    if (speakerMax) {
      uiOptions.speakerMaxCount = clampSpeakerCount(Number(speakerMax.value));
    }
    if (uiOptions.speakerMinCount > uiOptions.speakerMaxCount) {
      uiOptions.speakerMaxCount = uiOptions.speakerMinCount;
    }
    if (includeNames) {
      uiOptions.includeSpeakerNames = includeNames.checked;
    }
    if (diarMinSpeech) {
      uiOptions.diarizationMinSpeechSecs = clampDiarizationFloat(
        Number(diarMinSpeech.value),
        DIARIZATION_LIMITS.minSpeechSecs,
        0.25,
      );
    }
    if (diarSensitivity) {
      uiOptions.diarizationSensitivity = clampDiarizationFloat(
        Number(diarSensitivity.value),
        DIARIZATION_LIMITS.sensitivity,
        0.45,
      );
    }
    if (diarExpert) {
      uiOptions.diarizationExpertOpen = diarExpert.open;
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

  const renameSpeakerOnJob = (job: AudioSrtJob, speakerId: number, nextLabel: string): void => {
    const trimmed = nextLabel.trim();
    if (!trimmed) {
      return;
    }
    const speaker = job.speakers.find((entry) => entry.id === speakerId);
    if (!speaker || speaker.label === trimmed) {
      return;
    }
    job.srt = rewriteSrtSpeakerLabels(job.srt, speaker.label, trimmed);
    job.vtt = rewriteVttVoiceTags(job.vtt, speaker.label, trimmed);
    speaker.label = trimmed;
    const textarea = root.querySelector<HTMLTextAreaElement>("[data-srt-result-text]");
    if (textarea && selectedId === job.id) {
      textarea.value = job.srt;
    }
    syncPreview();
  };

  const mergeSpeakerOnJob = (job: AudioSrtJob, fromId: number, intoId: number): void => {
    const merged = mergeSpeakers(
      job.srt,
      job.vtt,
      job.speakers,
      job.cueSpeakerIds,
      fromId,
      intoId,
    );
    job.srt = merged.srt;
    job.vtt = merged.vtt;
    job.speakers = merged.speakers;
    job.cueSpeakerIds = merged.cueSpeakerIds;
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
      statusHint,
      diarizationAvailable,
    );
    bind();
    syncPreview();
  };

  decodeProgress.bindRepaint(paint);

  const bindFieldHelp = (container: HTMLElement | null): void => {
    if (!container) {
      return;
    }
    // Help sits inside <label> that wraps range inputs — block label activation
    // so clicking "?" does not flash-focus the slider.
    container.addEventListener("mousedown", (event) => {
      const target = event.target;
      if (!(target instanceof HTMLElement)) {
        return;
      }
      if (target.closest(".field-help-btn")) {
        event.preventDefault();
        event.stopPropagation();
      }
    });
    container.addEventListener("click", (event) => {
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
  };

  const ensureDiarizationModel = async (): Promise<
    { ok: true } | { ok: false; reason: "cancelled" | "unavailable" | "download" }
  > => {
    try {
      const status = await getDiarizationModelStatus();
      if (!status.available) {
        uiOptions.speakerDiarization = false;
        diarizationAvailable = false;
        paint();
        return { ok: false, reason: "unavailable" };
      }
      diarizationAvailable = true;
      if (status.exists) {
        return { ok: true };
      }
      const ok = await showConfirmDialog({
        message: t("tools.audioSrt.diarizationDownloadConfirm", {
          size: String(status.sizeMb || 12),
        }),
        confirmLabel: t("tools.audioSrt.diarizationDownload"),
      });
      if (!ok) {
        return { ok: false, reason: "cancelled" };
      }
      statusHint = t("tools.audioSrt.diarizationDownloading");
      paint();
      await downloadDiarizationModel();
      statusHint = null;
      paint();
      return { ok: true };
    } catch {
      statusHint = null;
      paint();
      return { ok: false, reason: "download" };
    }
  };

  const bind = (): void => {
    root.querySelector<HTMLButtonElement>("[data-pick-audio-srt]")?.addEventListener("click", () => {
      void pickAndEnqueue();
    });

    root.querySelector<HTMLButtonElement>("[data-copy-srt-result]")?.addEventListener("click", () => {
      void copyResult();
    });

    root.querySelector<HTMLButtonElement>("[data-save-srt]")?.addEventListener("click", () => {
      void saveResult("srt");
    });
    root.querySelector<HTMLButtonElement>("[data-save-vtt]")?.addEventListener("click", () => {
      void saveResult("vtt");
    });
    root.querySelector<HTMLButtonElement>("[data-save-srt-as]")?.addEventListener("click", () => {
      void saveResult("auto");
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
      "[data-opt-max-line], [data-opt-max-lines], [data-opt-offset], [data-opt-bom], [data-opt-dictation], [data-opt-smart-split], [data-opt-pause-split], [data-opt-reading-tail], [data-opt-max-cue-sec], [data-opt-min-cue-sec], [data-opt-speaker-diarization], [data-opt-speaker-count-mode], [data-opt-speaker-exact], [data-opt-speaker-min], [data-opt-speaker-max], [data-opt-include-speaker-names], [data-opt-diar-min-speech], [data-opt-diar-sensitivity]",
    ).forEach((element) => {
      const onUpdate = (): void => {
        const smartBefore = uiOptions.smartSplit;
        const diarBefore = uiOptions.speakerDiarization;
        const modeBefore = uiOptions.speakerCountMode;
        readUiOptionsFromDom();
        syncSrtSliderLabels(root);
        const structureChanged =
          (element.matches("[data-opt-smart-split]") && smartBefore !== uiOptions.smartSplit) ||
          (element.matches("[data-opt-speaker-diarization]") &&
            diarBefore !== uiOptions.speakerDiarization) ||
          (element.matches("[data-opt-speaker-count-mode]") &&
            modeBefore !== uiOptions.speakerCountMode);
        if (structureChanged) {
          if (
            element.matches("[data-opt-speaker-diarization]") &&
            uiOptions.speakerDiarization &&
            diarizationAvailable === false
          ) {
            uiOptions.speakerDiarization = false;
            paint();
            return;
          }
          paint();
          if (
            element.matches("[data-opt-speaker-diarization]") &&
            uiOptions.speakerDiarization
          ) {
            void ensureDiarizationModel().then((result) => {
              if (!result.ok && uiOptions.speakerDiarization) {
                uiOptions.speakerDiarization = false;
                if (result.reason === "unavailable") {
                  diarizationAvailable = false;
                }
                paint();
              }
            });
          }
        }
      };
      element.addEventListener("change", onUpdate);
      if (element instanceof HTMLInputElement && element.type === "range") {
        element.addEventListener("input", onUpdate);
      }
    });

    bindFieldHelp(root.querySelector<HTMLElement>(".audio-srt-advanced-body"));
    bindFieldHelp(root.querySelector<HTMLElement>("[data-audio-srt-diarization]"));

    root.querySelector<HTMLDetailsElement>("[data-audio-srt-advanced]")?.addEventListener("toggle", () => {
      readUiOptionsFromDom();
    });
    root
      .querySelector<HTMLDetailsElement>("[data-audio-srt-diarization-expert]")
      ?.addEventListener("toggle", () => {
        readUiOptionsFromDom();
      });

    root.querySelectorAll<HTMLInputElement>("[data-speaker-label]").forEach((input) => {
      const apply = (): void => {
        const speakerId = Number(input.dataset.speakerLabel);
        const job = selectedJob();
        if (!job || job.status !== "done" || !Number.isFinite(speakerId)) {
          return;
        }
        syncEditedSrtFromTextarea();
        renameSpeakerOnJob(job, speakerId, input.value);
        const swatchRow = root.querySelector(`[data-speaker-row="${speakerId}"]`);
        // Keep merge option labels in sync without full paint.
        root.querySelectorAll<HTMLSelectElement>("[data-speaker-merge-from]").forEach((select) => {
          const fromId = Number(select.dataset.speakerMergeFrom);
          if (fromId === speakerId) {
            return;
          }
          const option = select.querySelector<HTMLOptionElement>(`option[value="${speakerId}"]`);
          if (option) {
            option.textContent = input.value.trim() || option.textContent;
          }
        });
        void swatchRow;
      };
      input.addEventListener("change", apply);
      input.addEventListener("blur", apply);
      input.addEventListener("keydown", (event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          input.blur();
        }
      });
    });

    root.querySelectorAll<HTMLSelectElement>("[data-speaker-merge-from]").forEach((select) => {
      select.addEventListener("change", () => {
        const fromId = Number(select.dataset.speakerMergeFrom);
        const intoId = Number(select.value);
        const job = selectedJob();
        if (
          !job ||
          job.status !== "done" ||
          !Number.isFinite(fromId) ||
          !Number.isFinite(intoId) ||
          select.value === ""
        ) {
          return;
        }
        syncEditedSrtFromTextarea();
        mergeSpeakerOnJob(job, fromId, intoId);
        paint();
      });
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

  const defaultSubtitleName = (fileName: string, ext: "srt" | "vtt"): string => {
    const base = fileName.replace(/\.[^.\\/]+$/, "");
    return `${base || "subtitles"}.${ext}`;
  };

  const contentForFormat = (
    job: AudioSrtJob,
    format: "srt" | "vtt",
  ): string => {
    if (format === "vtt") {
      return job.vtt.trim() ? job.vtt : "";
    }
    return job.srt.trim() ? job.srt : "";
  };

  const formatFromPath = (path: string): "srt" | "vtt" => {
    const lower = path.replace(/\\/g, "/").toLowerCase();
    return lower.endsWith(".vtt") ? "vtt" : "srt";
  };

  const saveResult = async (mode: "srt" | "vtt" | "auto"): Promise<void> => {
    syncEditedSrtFromTextarea();
    const selected = selectedJob();
    if (!selected || (!selected.srt.trim() && !selected.vtt.trim())) {
      return;
    }
    readUiOptionsFromDom();
    try {
      const preferredExt = mode === "vtt" ? "vtt" : "srt";
      const targetPath = await pickSubtitleSavePath(
        defaultSubtitleName(selected.fileName, preferredExt),
      );
      if (!targetPath) {
        return;
      }
      const format = mode === "auto" ? formatFromPath(targetPath) : mode;
      const content = contentForFormat(selected, format);
      if (!content.trim()) {
        saveHint = t("tools.audioSrt.saveFailed");
        paint();
        return;
      }
      await saveSubtitleFile(targetPath, content, uiOptions.utf8Bom);
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
        vtt: "",
        errorKey: null,
        errorRaw: null,
        rewriteFallback: false,
        aiRewriteApplied: false,
        speakers: [],
        cueSpeakerIds: [],
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
            vtt: result.vtt ?? "",
            rewriteFallback: result.rewriteFallback,
            aiRewriteApplied: result.aiRewriteApplied,
            processingPhase: null,
            processingPercent: null,
            fileName: result.fileName || job.fileName,
            speakers: result.speakers ?? [],
            cueSpeakerIds: result.cueSpeakerIds ?? [],
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
        readUiOptionsFromDom();
        if (uiOptions.speakerDiarization) {
          const modelResult = await ensureDiarizationModel();
          if (!modelResult.ok) {
            if (modelResult.reason === "cancelled") {
              break;
            }
            applyError(
              next.id,
              modelResult.reason === "unavailable"
                ? "tools.audioSrt.diarizationUnavailable"
                : "tools.audioSrt.diarizationDownloadFailed",
            );
            paint();
            continue;
          }
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
        statusHint = null;
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
