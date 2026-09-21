import { t } from "../i18n";
import { iconSettings } from "./icons";
import { syncStatusOverlayPopoverPosition } from "./status-overlay-layout";

let panelOpen = false;

export function isStatusQuickSettingsOpen(): boolean {
  return panelOpen;
}

export function setStatusQuickSettingsOpen(open: boolean): void {
  panelOpen = open;
  if (open) {
    void import("./status-events").then((m) => m.setStatusEventsOpen(false));
  }
  syncStatusQuickSettingsDom();
}

function syncStatusQuickSettingsLayout(): void {
  const popover = document.querySelector<HTMLElement>("[data-status-quick-settings-popover]");
  if (!popover) {
    return;
  }
  syncStatusOverlayPopoverPosition(popover);
}

function syncStatusQuickSettingsDom(): void {
  const popover = document.querySelector<HTMLElement>("[data-status-quick-settings-popover]");
  const toggle = document.querySelector<HTMLButtonElement>("[data-toggle-status-quick-settings]");
  if (popover) {
    popover.hidden = !panelOpen;
    if (panelOpen) {
      syncStatusQuickSettingsLayout();
    }
  }
  if (toggle) {
    toggle.setAttribute("aria-expanded", panelOpen ? "true" : "false");
  }
}

export function renderStatusBarGearButton(): string {
  const label = t("status.openQuickSettings");
  return `
    <button
      type="button"
      class="status-gear-btn icon-btn"
      data-toggle-status-quick-settings
      aria-label="${escapeHtml(label)}"
      title="${escapeHtml(label)}"
      aria-expanded="${panelOpen ? "true" : "false"}"
      aria-controls="status-quick-settings-panel"
    >${iconSettings()}</button>`;
}

export function bindStatusQuickSettings(): void {
  bindQuickSettingsEscapeListener();
  bindQuickSettingsClickDelegation();
  syncStatusQuickSettingsDom();
}

let clickDelegationBound = false;

function bindQuickSettingsClickDelegation(): void {
  if (clickDelegationBound) {
    return;
  }
  clickDelegationBound = true;

  document.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;

    if (target.closest("[data-toggle-status-quick-settings]")) {
      event.stopPropagation();
      setStatusQuickSettingsOpen(!panelOpen);
      return;
    }

    if (target.closest("[data-close-status-quick-settings]")) {
      event.preventDefault();
      event.stopPropagation();
      setStatusQuickSettingsOpen(false);
      return;
    }

    const backdrop = target.closest<HTMLElement>("[data-status-quick-settings-popover]");
    if (backdrop && target === backdrop) {
      setStatusQuickSettingsOpen(false);
    }
  });
}

let escapeListenerBound = false;

function bindQuickSettingsEscapeListener(): void {
  if (escapeListenerBound) {
    return;
  }
  escapeListenerBound = true;
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !panelOpen) {
      return;
    }
    setStatusQuickSettingsOpen(false);
  });
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
