import { t } from "../i18n";
import { escapeHtml } from "../lib/html";

const STORAGE_KEY = "veyro.standard.onboarding.dismissed";

export function isStandardOnboardingDismissed(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function renderStandardOnboardingBanner(): string {
  if (isStandardOnboardingDismissed()) {
    return "";
  }

  return `
    <aside class="onboarding-banner" role="note" data-standard-onboarding>
      <p class="onboarding-banner-text">${escapeHtml(t("onboarding.standardHint"))}</p>
      <button
        type="button"
        class="onboarding-banner-dismiss"
        data-dismiss-onboarding
        aria-label="${escapeHtml(t("common.cancel"))}"
      >×</button>
    </aside>
  `;
}

export function bindStandardOnboardingBanner(root: ParentNode): void {
  root.querySelector<HTMLButtonElement>("[data-dismiss-onboarding]")?.addEventListener("click", () => {
    try {
      localStorage.setItem(STORAGE_KEY, "1");
    } catch {
      // Ignore storage failures.
    }

    root.querySelector<HTMLElement>("[data-standard-onboarding]")?.remove();
  });
}
