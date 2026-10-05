export interface OverlayDialogOptions<T> {
  innerHtml: string;
  dismissValue: T;
  focusSelector: string;
  bind: (overlay: HTMLElement, finish: (value: T) => void) => void;
}

/** Shared confirm-overlay shell. Callers keep their own dialog markup and buttons. */
export function presentOverlayDialog<T>(options: OverlayDialogOptions<T>): Promise<T> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "confirm-overlay";
    overlay.innerHTML = options.innerHtml;

    let settled = false;
    const finish = (value: T): void => {
      if (settled) {
        return;
      }
      settled = true;
      document.removeEventListener("keydown", onKeyDown);
      overlay.remove();
      resolve(value);
    };

    options.bind(overlay, finish);

    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) {
        finish(options.dismissValue);
      }
    });

    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        event.preventDefault();
        finish(options.dismissValue);
      }
    }
    document.addEventListener("keydown", onKeyDown);

    document.body.appendChild(overlay);
    overlay.querySelector<HTMLElement>(options.focusSelector)?.focus();
  });
}
