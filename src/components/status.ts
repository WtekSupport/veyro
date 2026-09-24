import type {

  ActivityLogEntry,

  DiagnosticsSnapshot,

  ErrorPayload,

  StatusSnapshot,

} from "../api";

import { t, translateActivity, translateState, translateError } from "../i18n";
import { renderToolsBadgeButton } from "./tools-list";
import { getState } from "../state";
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

export function patchLiveStatusUi(status: StatusSnapshot | null): void {
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
}

export function renderCompactStatusBar(status: StatusSnapshot | null): string {
  if (!status) {
    return `<div class="status-bar status-bar--compact"><span class="status-dot idle"></span><span class="status-text">${escapeHtml(t("status.loading"))}</span></div>`;
  }

  const label = translateState(status.state);

  return `
    <div class="status-bar status-bar--compact">
      <span class="${stateClass(status.state)}"></span>
      <span class="status-text">${escapeHtml(label)}</span>
    </div>
  `;
}

export function renderStatusBar(status: StatusSnapshot | null): string {

  if (!status) {

    return `<div class="status-bar"><span class="status-dot idle"></span><span class="status-text">${escapeHtml(t("status.loading"))}</span></div>`;

  }



  const label = translateState(status.state);
  const toolsOpen = getState().toolsPanelOpen;

  return `

    <div class="status-bar">

      <span class="${stateClass(status.state)}"></span>

      <span class="status-text">${escapeHtml(label)}</span>

      <div class="status-bar-trailing">
        ${renderToolsBadgeButton(toolsOpen)}
        ${renderStatusBarGearButton()}
      </div>

    </div>

  `;

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


