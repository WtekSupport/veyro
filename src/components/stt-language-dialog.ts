import { listTranscriptionLanguages, type TranscriptionLanguageInfo } from "../api";
import { getLocale, t } from "../i18n";
import {
  formatTranscriptionLanguageLabel,
  sortTranscriptionLanguagesForDisplay,
} from "../lib/transcription-language-display";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function renderLanguageOptions(languages: TranscriptionLanguageInfo[]): string {
  if (languages.length === 0) {
    return `<option value="">${escapeHtml(t("tools.stt.selectLanguage.noLanguages"))}</option>`;
  }
  const uiLocale = getLocale();
  return sortTranscriptionLanguagesForDisplay(languages, uiLocale)
    .map((language) => {
      const label = formatTranscriptionLanguageLabel(language.code, language.name, uiLocale);
      return `<option value="${escapeHtml(language.code)}">${escapeHtml(label)}</option>`;
    })
    .join("");
}

/** Blocks until the user picks a transcription language or cancels. */
export async function promptSttLanguageSelection(): Promise<string | null> {
  let languages: TranscriptionLanguageInfo[] = [];
  try {
    languages = await listTranscriptionLanguages();
  } catch {
    languages = [];
  }

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "confirm-overlay";
    overlay.innerHTML = `
      <div class="confirm-dialog confirm-dialog--stt-language" role="dialog" aria-modal="true">
        <h2 class="confirm-dialog-title">${escapeHtml(t("tools.stt.selectLanguage.title"))}</h2>
        <p class="confirm-dialog-message">${escapeHtml(t("tools.stt.selectLanguage.message"))}</p>
        <label class="field">
          <span>${escapeHtml(t("tools.stt.selectLanguage.languageLabel"))}</span>
          <select data-stt-language-select class="device-select">
            ${renderLanguageOptions(languages)}
          </select>
        </label>
        <div class="confirm-dialog-actions">
          <button type="button" class="btn btn-secondary" data-stt-language-cancel>
            ${escapeHtml(t("common.cancel"))}
          </button>
          <button type="button" class="btn btn-primary" data-stt-language-ok>
            ${escapeHtml(t("tools.stt.selectLanguage.confirm"))}
          </button>
        </div>
      </div>
    `;

    const cleanup = (language: string | null): void => {
      document.removeEventListener("keydown", onKeyDown);
      overlay.remove();
      resolve(language);
    };

    const select = overlay.querySelector<HTMLSelectElement>("[data-stt-language-select]");
    if (select && languages.length > 0) {
      select.value = languages[0]?.code ?? "";
    }

    overlay.querySelector<HTMLButtonElement>("[data-stt-language-cancel]")?.addEventListener(
      "click",
      () => cleanup(null),
    );
    overlay.querySelector<HTMLButtonElement>("[data-stt-language-ok]")?.addEventListener(
      "click",
      () => {
        const code = select?.value.trim() ?? "";
        if (!code) {
          cleanup(null);
          return;
        }
        cleanup(code);
      },
    );
    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) {
        cleanup(null);
      }
    });

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        cleanup(null);
      }
    };
    document.addEventListener("keydown", onKeyDown);

    document.body.appendChild(overlay);
    select?.focus();
  });
}
