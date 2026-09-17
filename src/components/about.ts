import logoUrl from "../assets/logo.png";

import type { AppInfo, ThirdPartyLicense } from "../api";
import { t } from "../i18n";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function renderThirdPartyLicenses(licenses: ThirdPartyLicense[]): string {
  if (licenses.length === 0) {
    return "";
  }

  const items = licenses
    .map(
      (entry) => `
        <li class="about-license-item">
          <div class="about-license-name">${escapeHtml(entry.name)}</div>
          <div class="about-license-meta">${escapeHtml(entry.license)}</div>
          <div class="about-license-copy">${escapeHtml(entry.copyright)}</div>
        </li>
      `,
    )
    .join("");

  return `
    <details class="about-licenses">
      <summary class="about-licenses-summary">${escapeHtml(t("about.thirdPartyTitle"))}</summary>
      <p class="about-body about-licenses-intro">${escapeHtml(t("about.thirdPartyIntro"))}</p>
      <ul class="about-licenses-list">${items}</ul>
    </details>
  `;
}

export function renderAboutWindow(info: AppInfo, licenses: ThirdPartyLicense[]): string {
  return `
    <main class="about-window">
      <div class="about-brand">
        <img class="about-logo" src="${logoUrl}" alt="${escapeHtml(t("app.title"))}" width="72" height="72" />
        <h1 class="about-title">${escapeHtml(t("app.title"))}</h1>
        <p class="about-version">${escapeHtml(t("about.version", { version: info.version_display }))}</p>
      </div>

      <section class="about-section">
        <h2 class="about-section-title">${escapeHtml(t("about.licenseTitle"))}</h2>
        <p class="about-body">${escapeHtml(t("about.licenseBody"))}</p>
      </section>

      ${renderThirdPartyLicenses(licenses)}

      <section class="about-section">
        <h2 class="about-section-title">${escapeHtml(t("about.publisherTitle"))}</h2>
        <p class="about-body">${escapeHtml(info.publisher)}</p>
      </section>

      <section class="about-section">
        <h2 class="about-section-title">${escapeHtml(t("about.copyrightTitle"))}</h2>
        <p class="about-body">${escapeHtml(info.copyright)}</p>
      </section>

      <section class="about-section about-section--muted">
        <p class="about-body about-disclaimer">${escapeHtml(t("about.disclaimer"))}</p>
      </section>

      <footer class="about-hwid">
        <span class="about-hwid-label">${escapeHtml(t("about.hwidTitle"))}</span>
        <code class="about-hwid-hash">${escapeHtml(info.hwid_hash)}</code>
      </footer>
    </main>
  `;
}
