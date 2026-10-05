import { iconSidebarCollapse, iconSidebarExpand } from "../components/icons";
import { escapeHtml } from "./html";

/** i18n strings for the shared tool file-browser sidebar chrome. */
export type ToolBrowserSidebarLabels = {
  ariaLabel: string;
  title: string;
  collapseLabel: string;
  expandLabel: string;
};

export function toolBrowserShellClass(collapsed: boolean): string {
  return `voice-files-tool voice-files-tool--with-browser${
    collapsed ? " voice-files-tool--sidebar-collapsed" : ""
  }`;
}

export function renderToolBrowserSidebarCollapsed(
  labels: ToolBrowserSidebarLabels,
): string {
  return `
    <aside class="voice-files-sidebar voice-files-sidebar--collapsed" aria-label="${escapeHtml(labels.ariaLabel)}">
      <button type="button" class="icon-btn voice-files-sidebar-toggle" data-toggle-sidebar
        aria-label="${escapeHtml(labels.expandLabel)}"
        title="${escapeHtml(labels.expandLabel)}">${iconSidebarExpand()}</button>
    </aside>`;
}

export function renderToolBrowserSidebarHead(
  labels: ToolBrowserSidebarLabels,
): string {
  return `
    <div class="voice-files-sidebar-head">
      <span class="voice-files-sidebar-title">${escapeHtml(labels.title)}</span>
      <button type="button" class="icon-btn" data-toggle-sidebar
        aria-label="${escapeHtml(labels.collapseLabel)}"
        title="${escapeHtml(labels.collapseLabel)}">${iconSidebarCollapse()}</button>
    </div>`;
}

export function renderToolBrowserSidebar(
  labels: ToolBrowserSidebarLabels,
  collapsed: boolean,
  bodyHtml: string,
): string {
  if (collapsed) {
    return renderToolBrowserSidebarCollapsed(labels);
  }
  return `
    <aside class="voice-files-sidebar" aria-label="${escapeHtml(labels.ariaLabel)}">
      ${renderToolBrowserSidebarHead(labels)}
      ${bodyHtml}
    </aside>`;
}

export function isSidebarToggleClick(target: EventTarget | null): boolean {
  return Boolean(
    (target as HTMLElement | null)?.closest?.("[data-toggle-sidebar]"),
  );
}

const STORAGE_PREFIX = "veyro.toolBrowser.sidebarCollapsed.";

export function loadToolBrowserSidebarCollapsed(
  toolId: string,
  defaultValue = false,
): boolean {
  try {
    const raw = localStorage.getItem(`${STORAGE_PREFIX}${toolId}`);
    if (raw === "1") {
      return true;
    }
    if (raw === "0") {
      return false;
    }
  } catch {
    /* ignore */
  }
  return defaultValue;
}

export function saveToolBrowserSidebarCollapsed(
  toolId: string,
  collapsed: boolean,
): void {
  try {
    localStorage.setItem(`${STORAGE_PREFIX}${toolId}`, collapsed ? "1" : "0");
  } catch {
    /* ignore */
  }
}
