import type {

  AiSkillInfo,

  AppSettings,

  Diagnostics,

  HomemakerLocalSetup,

  LlmModelDownloadProgress,

  SileroModelDownloadProgress,

  SileroTeModelStatus,

  SileroVadModelStatus,

  TextProcessingMode,

  WhisperModelDownloadProgress,

} from "../api";

import { resolveHomemakerHotkeySelection } from "./homemaker-hotkeys";
import { renderHomemakerSileroDownloads } from "./homemaker-silero-ui";
import { iconPlus } from "./icons";
import { renderWeakPcHomemakerSwitch } from "./settings";
import { t } from "../i18n";



export type HomemakerDataStorage = "cloud" | "local";



export function isHomemakerMode(settings: AppSettings | null): boolean {

  return settings?.ui_mode === "homemaker";

}



export function effectiveDataStorage(settings: AppSettings): HomemakerDataStorage {

  if (

    settings.transcription_provider !== "local" &&

    settings.text_rewrite_provider === "openai"

  ) {

    return "cloud";

  }

  return "local";

}



function escapeHtml(value: string): string {

  return value

    .replaceAll("&", "&amp;")

    .replaceAll("<", "&lt;")

    .replaceAll(">", "&gt;")

    .replaceAll('"', "&quot;");

}

function resolveHomemakerSkillDisplay(
  settings: AppSettings,
  aiSkills: AiSkillInfo[],
): { primary: string; secondary: string | null } {
  const active =
    aiSkills.find((skill) => skill.filename === settings.ai_rewrite_skill) ?? aiSkills[0] ?? null;
  if (settings.text_processing_mode === "custom_skill" && active) {
    return { primary: active.name, secondary: null };
  }
  if (active) {
    return {
      primary: t("homemaker.textModeCustomSkill"),
      secondary: active.name,
    };
  }
  return {
    primary: t("homemaker.textModeMore"),
    secondary: t("homemaker.textModeCustomSkillEmpty"),
  };
}



function renderDownloadProgress(

  progress: WhisperModelDownloadProgress | LlmModelDownloadProgress | null,

): string {

  if (!progress) {

    return "";

  }



  const percent =

    progress.percent != null

      ? `${Math.round(progress.percent)}%`

      : t("homemaker.downloading");



  const indeterminate = progress.percent == null;

  return `

    <div class="homemaker-download-progress">

      <div
        class="download-progress"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow="${progress.percent ?? 0}"
      >

        <div
          class="download-progress-bar ${indeterminate ? "is-indeterminate" : ""}"
          style="${indeterminate ? "" : `width: ${progress.percent}%;`}"
        ></div>

      </div>

      <p class="field-hint">${escapeHtml(percent)}</p>

    </div>

  `;

}



export function renderHomemakerSettings(

  settings: AppSettings,

  hasApiKey: boolean,

  localSetup: HomemakerLocalSetup | null,

  whisperModelDownload: WhisperModelDownloadProgress | null,

  llmModelDownload: LlmModelDownloadProgress | null,

  hotkeyPresets: readonly string[],

  aiSkills: AiSkillInfo[],

  configLoading = false,

  diagnostics: Diagnostics | null = null,

  sileroTeModel: SileroTeModelStatus | null = null,

  sileroVadModel: SileroVadModelStatus | null = null,

  sileroTeModelDownload: SileroModelDownloadProgress | null = null,

  sileroVadModelDownload: SileroModelDownloadProgress | null = null,

): string {

  const storage = effectiveDataStorage(settings);

  const textMode =
    settings.text_processing_mode === "basic"
      ? "original"
      : settings.text_processing_mode;

  const optimizationAvailable =

    storage === "cloud" ? hasApiKey : Boolean(localSetup && !localSetup.llm_download_needed);

  const customSkillAvailable = optimizationAvailable && aiSkills.length > 0;

  const skillDisplay = resolveHomemakerSkillDisplay(settings, aiSkills);

  const whisperReady = Boolean(localSetup && !localSetup.whisper_download_needed);

  const llmReady = Boolean(localSetup && !localSetup.llm_download_needed);

  const localReady = storage === "local" && whisperReady && llmReady;

  const selectedHotkey = resolveHomemakerHotkeySelection(

    settings.global_hotkey,

    settings.capslock_ptt,

    hotkeyPresets,

  );



  return `

    <section class="panel homemaker-panel">

      <form id="homemaker-form" class="homemaker-form${configLoading ? " homemaker-form--busy" : ""}" ${configLoading ? 'aria-busy="true"' : ""}>

        <div class="homemaker-card">

          <div class="homemaker-card-head">${escapeHtml(t("homemaker.hotkeyTitle"))}</div>

          <div class="homemaker-choice-row">

            ${hotkeyPresets

              .map(

                (hotkey) => `

              <label class="homemaker-choice-card">

                <input

                  type="radio"

                  name="homemaker_hotkey"

                  value="${escapeHtml(hotkey)}"

                  ${selectedHotkey === hotkey ? "checked" : ""}

                />

                <span>${escapeHtml(hotkey)}</span>

              </label>

            `,

              )

              .join("")}

          </div>

        </div>



        <div class="homemaker-card">

          <div class="homemaker-card-head">${escapeHtml(t("homemaker.storageTitle"))}</div>

          <div class="homemaker-choice-row">

            <label class="homemaker-choice-card">

              <input type="radio" name="homemaker_data_storage" value="cloud" ${storage === "cloud" ? "checked" : ""} />

              <span>${escapeHtml(t("homemaker.storageCloud"))}</span>

            </label>

            <label class="homemaker-choice-card">

              <input type="radio" name="homemaker_data_storage" value="local" ${storage === "local" ? "checked" : ""} />

              <span>${escapeHtml(t("homemaker.storageLocal"))}</span>

            </label>

          </div>

          ${

            storage === "cloud"

              ? `

            <div class="homemaker-card-extra" data-homemaker-api-key>

              <input

                id="homemaker-api-key"

                name="api_key"

                type="password"

                autocomplete="off"

                placeholder="${escapeHtml(t("settings.apiKeyPlaceholder"))}"

              />

              ${

                hasApiKey

                  ? ""

                  : `<p class="hint homemaker-inline-hint">${escapeHtml(t("homemaker.apiKeyRequired"))}</p>`

              }

            </div>

          `

              : `

            <div class="homemaker-card-extra homemaker-local-block" data-homemaker-local>

              ${

                localReady

                  ? `<p class="homemaker-ready">${escapeHtml(t("homemaker.localReady"))}</p>`

                  : ""

              }

              ${

                localSetup?.whisper_download_needed

                  ? `

                <button type="button" class="btn btn-primary btn-compact" data-download-whisper-homemaker ${whisperModelDownload ? "disabled" : ""}>

                  ${escapeHtml(t("homemaker.downloadWhisper"))}

                </button>

                ${renderDownloadProgress(whisperModelDownload)}

              `

                  : ""

              }

              ${

                localSetup?.llm_download_needed

                  ? `

                <button type="button" class="btn btn-primary btn-compact" data-download-llm-homemaker ${llmModelDownload ? "disabled" : ""}>

                  ${escapeHtml(t("homemaker.downloadLlm"))}

                </button>

                ${renderDownloadProgress(llmModelDownload)}

              `

                  : ""

              }

            </div>

          `

          }

        </div>



        <div class="homemaker-card">

          <div class="homemaker-card-head">${escapeHtml(t("settings.textMode"))}</div>

          <div class="homemaker-choice-row homemaker-choice-row--triple">

            <label class="homemaker-choice-card homemaker-choice-card--text">

              <input

                type="radio"

                name="text_processing_mode"

                value="original"

                ${textMode === "original" ? "checked" : ""}

              />

              <span>${escapeHtml(t("settings.textModeOriginal"))}</span>

            </label>

            <label class="homemaker-choice-card homemaker-choice-card--text">

              <input

                type="radio"

                name="text_processing_mode"

                value="optimization"

                ${textMode === "optimization" ? "checked" : ""}

                ${optimizationAvailable ? "" : "disabled"}

              />

              <span>${escapeHtml(t("settings.textModeOptimization"))}</span>

            </label>

            <label class="homemaker-choice-card homemaker-choice-card--text homemaker-choice-card--skill">

              <input

                type="radio"

                name="text_processing_mode"

                value="custom_skill"

                ${textMode === "custom_skill" ? "checked" : ""}

                ${customSkillAvailable ? "" : "disabled"}

              />

              <span class="homemaker-skill-choice">

                <span class="homemaker-skill-choice-primary">${escapeHtml(skillDisplay.primary)}</span>

                ${

                  skillDisplay.secondary

                    ? `<span class="homemaker-skill-choice-secondary">${escapeHtml(skillDisplay.secondary)}</span>`

                    : ""

                }

              </span>

              <button

                type="button"

                class="homemaker-skill-catalog-badge"

                data-open-skill-catalog

                title="${escapeHtml(t("settings.aiSkillBrowseCatalog"))}"

                aria-label="${escapeHtml(t("settings.aiSkillBrowseCatalog"))}"

              >${iconPlus()}</button>

            </label>

          </div>

        </div>

        ${renderHomemakerSileroDownloads(
          diagnostics,
          sileroTeModel,
          sileroVadModel,
          sileroTeModelDownload,
          sileroVadModelDownload,
        )}

        ${renderWeakPcHomemakerSwitch(settings.weak_pc_mode ?? false)}

      </form>

    </section>

  `;

}



export function readHomemakerForm(form: HTMLFormElement): {

  homemaker_data_storage: HomemakerDataStorage;

  text_processing_mode: TextProcessingMode;

  global_hotkey: string;

  api_key: string;

} {

  const storage =

    (form.querySelector<HTMLInputElement>('input[name="homemaker_data_storage"]:checked')?.value ??

      "local") as HomemakerDataStorage;

  const textMode =

    (form.querySelector<HTMLInputElement>('input[name="text_processing_mode"]:checked')?.value ??

      "original") as TextProcessingMode;

  const apiKey = form.querySelector<HTMLInputElement>('input[name="api_key"]')?.value ?? "";

  const hotkey =

    form.querySelector<HTMLInputElement>('input[name="homemaker_hotkey"]:checked')?.value ??

    "CapsLock";



  return {

    homemaker_data_storage: storage,

    text_processing_mode: textMode,

    global_hotkey: hotkey,

    api_key: apiKey.trim(),

  };

}

