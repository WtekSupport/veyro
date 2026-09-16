import { t } from "../i18n";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function renderHomemakerConfigBanner(): string {
  return `
    <div class="homemaker-config-banner" role="status" aria-live="polite">
      <span class="homemaker-config-spinner" aria-hidden="true"></span>
      <span class="homemaker-config-text">${escapeHtml(t("homemaker.detectingConfig"))}</span>
    </div>
  `;
}
