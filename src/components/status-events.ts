import type { ActivityLogEntry } from "../api";
import { t } from "../i18n";
import { renderActivityLogListOnly } from "./status";
import { syncStatusOverlayPopoverPosition } from "./status-overlay-layout";

let panelOpen = false;

export function isStatusEventsOpen(): boolean {
  return panelOpen;
}

export function setStatusEventsOpen(open: boolean): void {
  panelOpen = open;
  if (open) {
    void import("./status-quick-settings").then((m) => m.setStatusQuickSettingsOpen(false));
  }
  syncStatusEventsDom();
}

function syncStatusEventsDom(): void {
  const popover = document.querySelector<HTMLElement>("[data-status-events-popover]");
  const toggle = document.querySelector<HTMLButtonElement>("[data-toggle-status-events]");
  if (popover) {
    popover.hidden = !panelOpen;
    if (panelOpen) {
      syncStatusOverlayPopoverPosition(popover);
    }
  }
  if (toggle) {
    toggle.setAttribute("aria-expanded", panelOpen ? "true" : "false");
  }
}

export function renderStatusEventsPopover(activityLog: ActivityLogEntry[]): string {
  return `
    <div class="status-quick-settings" data-status-events-popover ${panelOpen ? "" : "hidden"}>
      <div
        class="status-quick-settings-panel"
        id="status-events-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="status-events-title"
      >
        <header class="status-quick-settings-header">
          <h4 class="status-quick-settings-title" id="status-events-title">
            ${escapeHtml(t("status.logExpand"))}
          </h4>
          <div class="status-overlay-header-actions">
            <button type="button" class="activity-log-clear" data-clear-log>
              ${escapeHtml(t("activity.clear"))}
            </button>
            <button
              type="button"
              class="status-quick-settings-close icon-btn"
              data-close-status-events
              aria-label="${escapeHtml(t("common.close"))}"
              title="${escapeHtml(t("common.close"))}"
            >×</button>
          </div>
        </header>
        <div class="status-quick-settings-body status-events-body">
          ${renderActivityLogListOnly(activityLog)}
        </div>
      </div>
    </div>
  `;
}

export function renderStatusEventsTrigger(): string {
  return `
    <button
      type="button"
      class="status-events-trigger"
      data-toggle-status-events
      aria-expanded="${panelOpen ? "true" : "false"}"
      aria-controls="status-events-panel"
    >${escapeHtml(t("status.logExpand"))}</button>
  `;
}

export function bindStatusEventsPanel(): void {
  bindEventsEscapeListener();
  bindEventsClickDelegation();
  syncStatusEventsDom();
}

let clickDelegationBound = false;

function bindEventsClickDelegation(): void {
  if (clickDelegationBound) {
    return;
  }
  clickDelegationBound = true;

  document.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;

    if (target.closest("[data-toggle-status-events]")) {
      event.preventDefault();
      event.stopPropagation();
      setStatusEventsOpen(!panelOpen);
      return;
    }

    if (target.closest("[data-close-status-events]")) {
      event.preventDefault();
      event.stopPropagation();
      setStatusEventsOpen(false);
      return;
    }

    const backdrop = target.closest<HTMLElement>("[data-status-events-popover]");
    if (backdrop && target === backdrop) {
      setStatusEventsOpen(false);
    }
  });
}

let escapeListenerBound = false;

function bindEventsEscapeListener(): void {
  if (escapeListenerBound) {
    return;
  }
  escapeListenerBound = true;
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !panelOpen) {
      return;
    }
    setStatusEventsOpen(false);
  });
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
