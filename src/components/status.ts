import type {

  ActivityLogEntry,

  DiagnosticsSnapshot,

  ErrorPayload,

  StatusSnapshot,

} from "../api";

import { t, translateActivity, translateState, translateError } from "../i18n";
import { renderStatusBarGearButton } from "./status-quick-settings";



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

function livePartialMarkup(
  partialTranscript: string | null,
  compact: boolean,
): string {
  const trimmed = partialTranscript?.trim();
  if (!trimmed) {
    return "";
  }
  const tag = compact ? "span" : "p";
  const max = compact ? 80 : 120;
  return `<${tag} class="status-partial" aria-live="polite">${escapeHtml(truncatePartial(trimmed, max))}</${tag}>`;
}

export function patchLiveStatusUi(
  status: StatusSnapshot | null,
  partialTranscript: string | null = null,
): void {
  if (!status) {
    return;
  }

  const label = translateState(status.state);
  const klass = stateClass(status.state);

  document.querySelectorAll(".status-bar").forEach((bar) => {
    const dot = bar.querySelector<HTMLElement>(".status-dot");
    if (dot) {
      dot.className = klass;
    }
    const text = bar.querySelector(".status-text");
    if (text) {
      text.textContent = label;
    }
  });

  const compactPartial = livePartialMarkup(partialTranscript, true);
  document.querySelectorAll(".status-bar--compact").forEach((bar) => {
    const existing = bar.querySelector(".status-partial");
    if (existing) {
      existing.remove();
    }
    if (compactPartial) {
      bar.insertAdjacentHTML("beforeend", compactPartial);
    }
  });

  const fullPartial = livePartialMarkup(partialTranscript, false);
  document.querySelectorAll(".status-bar:not(.status-bar--compact)").forEach((bar) => {
    let sibling = bar.nextElementSibling;
    if (sibling?.classList.contains("status-partial")) {
      sibling.remove();
      sibling = bar.nextElementSibling;
    }
    if (fullPartial) {
      bar.insertAdjacentHTML("afterend", fullPartial);
    }
  });
}

export function renderCompactStatusBar(
  status: StatusSnapshot | null,
  partialTranscript: string | null = null,
): string {
  if (!status) {
    return `<div class="status-bar status-bar--compact"><span class="status-dot idle"></span><span class="status-text">${escapeHtml(t("status.loading"))}</span></div>`;
  }

  const label = translateState(status.state);
  const partial = livePartialMarkup(partialTranscript, true);

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
  const partial = livePartialMarkup(partialTranscript, false);

  return `

    <div class="status-bar">

      <span class="${stateClass(status.state)}"></span>

      <span class="status-text">${escapeHtml(label)}</span>

      ${renderStatusBarGearButton()}

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

export function renderActivityLogListOnly(entries: ActivityLogEntry[]): string {
  return `<ul class="activity-log-list">${activityLogListHtml(entries)}</ul>`;
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



function escapeHtml(value: string): string {

  return value

    .replaceAll("&", "&amp;")

    .replaceAll("<", "&lt;")

    .replaceAll(">", "&gt;")

    .replaceAll('"', "&quot;");

}


