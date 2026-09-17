import type {

  ActivityLogEntry,

  DiagnosticsSnapshot,

  ErrorPayload,

  StatusSnapshot,

} from "../api";

import {

  iconDiagAudio,

  iconDiagCallbacks,

  iconDiagInject,

  iconDiagKey,

  iconDiagLlm,

  iconDiagMic,

  iconDiagRewrite,

  iconDiagStt,

  iconDiagText,

  iconDiagWhisper,

} from "./icons";

import { t, translateActivity, translateState, translateError, whisperBackendLabel } from "../i18n";



const ERROR_STATES = new Set<StatusSnapshot["state"]>([

  "error",

  "permission_required",

  "microphone_unavailable",

  "network_unavailable",

]);



export function shouldShowErrorRecovery(

  status: StatusSnapshot | null,

  lastError: ErrorPayload | null,

): boolean {

  if (lastError) {

    return true;

  }

  if (status?.last_error) {

    return true;

  }

  return status ? ERROR_STATES.has(status.state) : false;

}



export function renderErrorBanner(

  status: StatusSnapshot | null,

  lastError: ErrorPayload | null,

): string {

  if (!shouldShowErrorRecovery(status, lastError)) {

    return "";

  }



  const message =

    status?.last_error ??

    (lastError

      ? translateError(lastError.code, lastError.message)

      : t("errors.engineStuck"));



  return `

    <div class="error-banner">

      <p class="error-text">${escapeHtml(message)}</p>

      <button type="button" class="error-dismiss" data-recover-engine>${escapeHtml(t("errors.recoverAction"))}</button>

    </div>

  `;

}



function stateClass(state: StatusSnapshot["state"]): string {

  switch (state) {

    case "ready":

      return "status-dot ready";

    case "listening":

      return "status-dot listening";

    case "processing":

    case "transcribing":

    case "injecting":

      return "status-dot processing";

    case "error":

      return "status-dot error";

    default:

      return "status-dot idle";

  }

}



export function renderCompactStatusBar(
  status: StatusSnapshot | null,
  partialTranscript: string | null = null,
): string {
  if (!status) {
    return `<div class="status-bar status-bar--compact"><span class="status-dot idle"></span><span class="status-text">${escapeHtml(t("status.loading"))}</span></div>`;
  }

  const label = translateState(status.state);
  const partial =
    status.state === "listening"
      ? `<span class="status-partial status-listening-indicator" aria-hidden="true"><span class="listening-dots"></span></span>`
      : partialTranscript?.trim()
        ? `<span class="status-partial" aria-live="polite">${escapeHtml(truncatePartial(partialTranscript, 80))}</span>`
        : "";

  return `
    <div class="status-bar status-bar--compact">
      <span class="${stateClass(status.state)}"></span>
      <span class="status-text">${escapeHtml(label)}</span>
      ${partial}
    </div>
  `;
}

export function renderStatusBar(
  status: StatusSnapshot | null,
  partialTranscript: string | null = null,
): string {

  if (!status) {

    return `<div class="status-bar"><span class="status-dot idle"></span><span class="status-text">${escapeHtml(t("status.loading"))}</span></div>`;

  }



  const label = translateState(status.state);
  const partial =
    status.state === "listening"
      ? `<p class="status-partial status-listening-indicator" aria-hidden="true"><span class="listening-dots"></span></p>`
      : partialTranscript?.trim()
        ? `<p class="status-partial" aria-live="polite">${escapeHtml(truncatePartial(partialTranscript, 120))}</p>`
        : "";

  return `

    <div class="status-bar">

      <span class="${stateClass(status.state)}"></span>

      <span class="status-text">${escapeHtml(label)}</span>

      <span class="status-meta">${status.state === "ready" ? t("status.ready") : t("status.live")}</span>

    </div>

    ${partial}

  `;

}

function truncatePartial(text: string, maxChars: number): string {
  const trimmed = text.trim();
  if (trimmed.length <= maxChars) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxChars - 1)}…`;
}



export function renderUsageHint(

  status: StatusSnapshot | null,

  diagnostics: DiagnosticsSnapshot | null,

): string {

  if (!status) {

    return "";

  }



  if (status.state === "initializing") {

    return `<p class="panel-note">${escapeHtml(t("status.starting"))}</p>`;

  }



  if (status.push_to_talk) {
    return "";
  }



  if (status.state === "listening") {

    return `<p class="panel-note">${escapeHtml(t("status.listening"))}</p>`;

  }



  if (diagnostics?.audio_capturing || status.state === "ready") {

    return `<p class="panel-note">${escapeHtml(t("status.continuous"))}</p>`;

  }



  return `<p class="panel-note">${escapeHtml(t("status.waitingMic"))}</p>`;

}



export function renderCompactDiagnostics(

  diagnostics: DiagnosticsSnapshot | null,

): string {

  if (!diagnostics) {

    return "";

  }



  const audioLabel = audioStatusLabel(diagnostics);

  const micLabel = shorten(diagnostics.microphone_device ?? t("diag.default"), 32);

  const whisperLabel = whisperDiagLabel(diagnostics);

  const textModeLabel = textModeDiagLabel(diagnostics.text_processing_mode);

  const sttLabel = sttProviderDiagLabel(diagnostics);

  const rewriteLabel = rewriteProviderDiagLabel(diagnostics);

  const llmLabel = llmDiagLabel(diagnostics);



  const icons = [

    diagIconMark(

      iconDiagAudio(),

      diagnostics.audio_running || diagnostics.audio_capturing,

      diagTooltip(t("diag.audio"), audioLabel),

    ),

    diagIconMark(

      iconDiagInject(),

      diagnostics.injection_available,

      diagTooltip(t("diag.inject"), diagnostics.injection_available ? t("diag.ok") : t("diag.na")),

    ),

    diagIconMark(

      iconDiagKey(),

      diagnostics.has_api_key,

      diagTooltip(t("diag.api"), diagnostics.has_api_key ? t("diag.set") : t("diag.missing")),

    ),

    diagIconMark(

      iconDiagMic(),

      diagnostics.microphone_device !== null,

      diagTooltip(t("diag.mic"), micLabel),

    ),

    diagIconMark(

      iconDiagCallbacks(),

      diagnostics.callbacks_enabled,

      diagTooltip(t("diag.callbacks"), diagnostics.callbacks_enabled ? t("diag.on") : t("diag.off")),

    ),

    diagIconMark(

      iconDiagWhisper(),

      diagnostics.whisper_local_compiled &&

        diagnostics.transcription_provider === "local" &&

        diagnostics.whisper_loaded,

      diagTooltip(t("diag.whisper"), whisperLabel),

    ),

    diagIconMark(

      iconDiagText(),

      diagnostics.text_processing_mode === "optimization" ||

        diagnostics.text_processing_mode === "custom_skill",

      diagTooltip(t("diag.textMode"), textModeLabel),

    ),

    diagIconMark(

      iconDiagStt(),

      sttProviderActive(diagnostics),

      diagTooltip(t("diag.sttProvider"), sttLabel),

    ),

    diagIconMark(

      iconDiagRewrite(),

      rewriteProviderActive(diagnostics),

      diagTooltip(t("diag.rewriteProvider"), rewriteLabel),

    ),

    diagIconMark(

      iconDiagLlm(),

      diagnostics.local_llm_compiled &&

        diagnostics.text_rewrite_provider === "local" &&

        diagnostics.llm_loaded,

      diagTooltip(t("diag.llm"), llmLabel),

    ),

  ];



  return `<div class="diag-strip" role="list" aria-label="${escapeHtml(t("diag.stripLabel"))}">${icons.join("")}</div>`;

}



function diagIconMark(icon: string, active: boolean, title: string): string {

  const stateClass = active ? " is-active" : "";

  return `<span class="diag-icon${stateClass}" role="listitem" title="${escapeHtml(title)}" aria-label="${escapeHtml(title)}">${icon}</span>`;

}



function diagTooltip(label: string, value: string): string {

  return `${label}: ${value}`;

}



function sttProviderActive(diagnostics: DiagnosticsSnapshot): boolean {

  if (diagnostics.transcription_provider === "local") {

    return diagnostics.whisper_local_compiled;

  }

  return diagnostics.has_api_key;

}



function rewriteProviderActive(diagnostics: DiagnosticsSnapshot): boolean {

  if (diagnostics.text_rewrite_provider === "local") {

    return diagnostics.local_llm_compiled && diagnostics.llm_loaded;

  }

  return diagnostics.has_api_key;

}



function activityLogListHtml(entries: ActivityLogEntry[]): string {
  if (entries.length === 0) {
    return `<li class="activity-log-empty">${escapeHtml(t("activity.empty"))}</li>`;
  }

  return [...entries]
    .reverse()
    .map((entry) => {
      const args =
        entry.message_args && typeof entry.message_args === "object"
          ? (entry.message_args as Record<string, unknown>)
          : {};
      const message = translateActivity(entry.message_key, args);
      return `
              <li class="activity-log-item activity-log-${entry.level}">
                <time class="activity-log-time">${formatLogTime(entry.timestamp_ms)}</time>
                <span class="activity-log-message">${escapeHtml(message)}</span>
              </li>
            `;
    })
    .join("");
}

export function updateActivityLogDom(entries: ActivityLogEntry[]): void {
  const list = document.querySelector<HTMLUListElement>(".activity-log-list");
  if (!list) {
    return;
  }
  list.innerHTML = activityLogListHtml(entries);
}

export function renderActivityLog(

  entries: ActivityLogEntry[],

  options: { showTitle?: boolean } = {},

): string {

  const showTitle = options.showTitle ?? false;

  const rows = activityLogListHtml(entries);



  const header = showTitle

    ? `<div class="activity-log-header">

        <h2 class="activity-log-title">${escapeHtml(t("activity.title"))}</h2>

        <button type="button" class="activity-log-clear" data-clear-log>${escapeHtml(t("activity.clear"))}</button>

      </div>`

    : `<div class="activity-log-header activity-log-header--actions-only">

        <button type="button" class="activity-log-clear" data-clear-log>${escapeHtml(t("activity.clear"))}</button>

      </div>`;



  return `

    <section class="activity-log-panel">

      ${header}

      <ul class="activity-log-list">${rows}</ul>

    </section>

  `;

}



function formatLogTime(timestampMs: number): string {

  const date = new Date(timestampMs);

  return date.toLocaleTimeString(undefined, {

    hour: "2-digit",

    minute: "2-digit",

    second: "2-digit",

  });

}



const TEXT_MODE_DIAG_KEYS: Record<string, import("../i18n/locales/en").MessageKey> = {

  original: "settings.textModeOriginal",

  basic: "settings.textModeBasic",

  optimization: "settings.textModeOptimization",

  custom_skill: "settings.textModeCustomSkill",

};



function sttProviderDiagLabel(diagnostics: DiagnosticsSnapshot): string {

  if (diagnostics.transcription_provider === "local") {

    return diagnostics.whisper_local_compiled

      ? t("settings.transcriptionProviderLocal")

      : t("diag.sttLocalMissing");

  }

  return t("settings.transcriptionProviderOpenai");

}



function textModeDiagLabel(mode: string): string {

  const key = TEXT_MODE_DIAG_KEYS[mode];

  return key ? t(key) : mode;

}



function whisperDiagLabel(diagnostics: DiagnosticsSnapshot): string {

  if (!diagnostics.whisper_local_compiled) {

    return t("diag.whisperLocalMissing");

  }

  if (diagnostics.transcription_provider === "local" && !diagnostics.whisper_loaded) {

    return t("diag.memoryUnloaded");

  }

  return whisperBackendLabel(diagnostics.whisper_backend);

}



function rewriteProviderDiagLabel(diagnostics: DiagnosticsSnapshot): string {

  if (diagnostics.text_rewrite_provider === "local") {

    return t("settings.textRewriteProviderLocal");

  }

  return t("settings.textRewriteProviderOpenai");

}



function llmDiagLabel(diagnostics: DiagnosticsSnapshot): string {

  if (!diagnostics.local_llm_compiled) {

    return t("diag.llmNotCompiled");

  }

  if (diagnostics.text_rewrite_provider !== "local") {

    return t("diag.off");

  }

  return diagnostics.llm_loaded ? t("diag.llmReady") : t("diag.llmMissing");

}



function audioStatusLabel(diagnostics: DiagnosticsSnapshot): string {

  if (diagnostics.audio_capturing) {

    return t("diag.capturing");

  }

  if (diagnostics.audio_running && diagnostics.push_to_talk) {

    return t("diag.ptt", { hotkey: diagnostics.hotkey });

  }

  if (diagnostics.audio_running) {

    return t("diag.live");

  }

  return t("diag.idle");

}



function shorten(value: string, max: number): string {

  return value.length > max ? `${value.slice(0, max - 1)}…` : value;

}



function escapeHtml(value: string): string {

  return value

    .replaceAll("&", "&amp;")

    .replaceAll("<", "&lt;")

    .replaceAll(">", "&gt;")

    .replaceAll('"', "&quot;");

}


