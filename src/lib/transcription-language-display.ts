import type { TranscriptionLanguageInfo } from "../api";
import type { UiLocale } from "../i18n";

export function formatTranscriptionLanguageLabel(
  code: string,
  fallbackName: string,
  uiLocale: UiLocale,
): string {
  try {
    const locale = uiLocale === "ru" ? "ru" : "en";
    const display = new Intl.DisplayNames([locale], { type: "language" });
    const intlCode = code === "yue" ? "yue" : code;
    const localized = display.of(intlCode);
    if (localized && localized.toLowerCase() !== code.toLowerCase()) {
      return localized;
    }
  } catch {
    // Intl.DisplayNames may be unavailable in older WebView2 builds.
  }
  return fallbackName;
}

/** Alphabetical by visible label; caller keeps "auto" as the first option. */
export function sortTranscriptionLanguagesForDisplay(
  languages: TranscriptionLanguageInfo[],
  uiLocale: UiLocale,
): TranscriptionLanguageInfo[] {
  const locale = uiLocale === "ru" ? "ru" : "en";
  return [...languages].sort((left, right) =>
    formatTranscriptionLanguageLabel(left.code, left.name, uiLocale).localeCompare(
      formatTranscriptionLanguageLabel(right.code, right.name, uiLocale),
      locale,
      { sensitivity: "base" },
    ),
  );
}
