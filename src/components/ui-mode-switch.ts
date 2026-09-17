import { updateSettings, type UiMode } from "../api";
import { t } from "../i18n";
import { showConfirmDialog } from "./confirm-dialog";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function renderUiModeLink(currentMode: UiMode): string {
  const nextMode: UiMode = currentMode === "homemaker" ? "expert" : "homemaker";
  const label =
    nextMode === "expert" ? t("uiMode.switchToExpert") : t("uiMode.switchToStandard");

  return `
    <button
      type="button"
      class="ui-mode-link"
      data-ui-mode-switch="${nextMode}"
      aria-label="${escapeHtml(label)}"
    >${escapeHtml(label)}</button>
  `;
}

async function switchUiMode(nextMode: UiMode, currentMode: UiMode): Promise<void> {
  if (nextMode === currentMode) {
    return;
  }

  if (nextMode === "expert") {
    const confirmed = await showConfirmDialog({
      message: t("uiMode.switchToExpertConfirm"),
    });
    if (!confirmed) {
      return;
    }
  }

  await updateSettings({ ui_mode: nextMode });
  window.location.reload();
}

export function bindUiModeSwitch(root: ParentNode, currentMode: UiMode): void {
  root.querySelectorAll<HTMLButtonElement>("[data-ui-mode-switch]").forEach((button) => {
    button.addEventListener("click", () => {
      const nextMode = button.dataset.uiModeSwitch as UiMode | undefined;
      if (!nextMode) {
        return;
      }

      void switchUiMode(nextMode, currentMode).catch(() => {
        // Settings update failed — keep current UI.
      });
    });
  });
}
