import { t } from "../i18n";

export interface ConfirmDialogOptions {
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
}

export function showConfirmDialog(options: ConfirmDialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "confirm-overlay";
    overlay.innerHTML = `
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
    `;

    const cleanup = (result: boolean): void => {
      document.removeEventListener("keydown", onKeyDown);
      overlay.remove();
      resolve(result);
    };

    overlay.querySelector<HTMLButtonElement>("[data-confirm-cancel]")?.addEventListener("click", () => {
      cleanup(false);
    });
    overlay.querySelector<HTMLButtonElement>("[data-confirm-ok]")?.addEventListener("click", () => {
      cleanup(true);
    });
    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) {
        cleanup(false);
      }
    });

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        cleanup(false);
      }
    };
    document.addEventListener("keydown", onKeyDown);

    document.body.appendChild(overlay);
    overlay.querySelector<HTMLButtonElement>("[data-confirm-ok]")?.focus();
  });
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
