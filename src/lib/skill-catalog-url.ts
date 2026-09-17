import type { UiLocale } from "../i18n";

export function skillCatalogUrl(locale: UiLocale = "en"): string {
  const base = locale === "ru" ? "https://aistructedit.com/ru" : "https://aistructedit.com";
  return `${base}/catalog?q=veyro`;
}
