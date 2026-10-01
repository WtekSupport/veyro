import { t } from "../i18n";
import { escapeHtml } from "../lib/html";
import { presentOverlayDialog } from "../lib/overlay-dialog";

export interface ConfirmDialogOptions {
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
}

export function showConfirmDialog(options: ConfirmDialogOptions): Promise<boolean> {
  return presentOverlayDialog({
    dismissValue: false,
    focusSelector: "[data-confirm-ok]",
    innerHtml: `
      <div class="confirm-dialog" role="dialog" aria-modal="true">
        <p class="confirm-dialog-message">${escapeHtml(options.message)}</p>
        <div class="confirm-dialog-actions">
          <button type="button" class="btn btn-secondary" data-confirm-cancel>
            ${escapeHtml(options.cancelLabel ?? t("common.cancel"))}
          </button>
          <button type="button" class="btn btn-primary" data-confirm-ok>
            ${escapeHtml(options.confirmLabel ?? t("common.continue"))}
          </button>
        </div>
      </div>
    `,
    bind: (overlay, finish) => {
      overlay.querySelector<HTMLButtonElement>("[data-confirm-cancel]")?.addEventListener("click", () => {
        finish(false);
      });
      overlay.querySelector<HTMLButtonElement>("[data-confirm-ok]")?.addEventListener("click", () => {
        finish(true);
      });
    },
  });
}
