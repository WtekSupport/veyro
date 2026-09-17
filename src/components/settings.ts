import type {
  ActivityLogEntry,
  AiSkillInfo,
  AppSettings,
  DiagnosticsSnapshot,
  InjectionMode,
  LlmModelDownloadProgress,
  LlmModelInfo,
  LlmModelKind,
  TextProcessingMode,
  TextRewriteProvider,
  TranscriptionLanguageInfo,
  UiLocale,
  WhisperModelDownloadProgress,
  WhisperModelInfo,
  WhisperModelKind,
} from "../api";
import type { SettingsTab } from "../state";
import { t, whisperBackendLabel } from "../i18n";
import type { MessageKey } from "../i18n/locales/en";
import { iconFolder, iconImport, iconPlus, iconTrash } from "./icons";
import { renderMicMeter } from "./mic-meter";
import { renderVadThresholdPanel } from "./vad-threshold-panel";
import type { VadThresholdMode } from "../api";
import { renderActivityLog } from "./status";

export interface SettingsFormValues {
  enabled: boolean;
  global_hotkey: string;
  push_to_talk: boolean;
  ptt_hold: boolean;
  live_dictation_field_indicator: boolean;
  hotkey_game_mode: boolean;
  hotkey_block_system: boolean;
  microphone_device: string;
  language: string;
  injection_mode: InjectionMode;
  text_processing_mode: TextProcessingMode;
  spoken_punctuation: boolean;
  auto_punctuation_from_pauses: boolean;
  numbers_as_words: boolean;
  emulate_enter: boolean;
  enter_trigger_phrase: string;
  start_on_boot: boolean;
  capslock_ptt: boolean;
  check_updates_on_startup: boolean;
  show_notifications: boolean;
  silence_timeout_ms: number;
  ui_locale: UiLocale;
  transcription_provider: string;
  local_whisper_model: WhisperModelKind;
  local_whisper_models_dir: string;
  whisper_model_exists: boolean;
  local_whisper_use_gpu: boolean;
  local_whisper_beam_size: number;
  text_rewrite_provider: TextRewriteProvider;
  local_llm_model: LlmModelKind;
  local_llm_models_dir: string;
  llm_model_exists: boolean;
  local_llm_use_gpu: boolean;
  ai_rewrite_skill: string;
  whisper_prompt_prefix: string;
  transcription_dictionary_path: string;
  audio_preprocess_enabled: boolean;
  audio_noise_reduction_enabled: boolean;
  vad_pre_speech_buffer_ms: number;
  vad_minimum_speech_ms: number;
  vad_maximum_segment_ms: number;
  vad_threshold_mode: VadThresholdMode;
  vad_voice_threshold_percent: number;
  vad_auto_threshold_percent: number;
  api_key: string;
  has_api_key: boolean;
}

const WHISPER_MODEL_LABELS: Record<WhisperModelKind, MessageKey> = {
  base: "settings.whisperModelBase",
  small: "settings.whisperModelSmall",
  medium: "settings.whisperModelMedium",
  large_v3_turbo: "settings.whisperModelLargeV3Turbo",
  large_v3: "settings.whisperModelLargeV3",
};

const WHISPER_MODEL_NAMES: Record<WhisperModelKind, MessageKey> = {
  base: "settings.whisperModelNameBase",
  small: "settings.whisperModelNameSmall",
  medium: "settings.whisperModelNameMedium",
  large_v3_turbo: "settings.whisperModelNameLargeV3Turbo",
  large_v3: "settings.whisperModelNameLargeV3",
};

function whisperModelHintKey(model: WhisperModelKind): MessageKey | null {
  if (model === "large_v3_turbo") {
    return "settings.whisperModelTurboHint";
  }
  if (model === "large_v3") {
    return "settings.whisperModelLargeHint";
  }
  return null;
}

const TEXT_MODE_HINT_KEYS: Record<TextProcessingMode, MessageKey> = {
  original: "settings.textModeHintOriginal",
  basic: "settings.textModeHintBasic",
  optimization: "settings.textModeHintOptimization",
  custom_skill: "settings.textModeHintCustomSkill",
};

const LLM_MODEL_LABELS: Record<LlmModelKind, MessageKey> = {
  qwen3_4b: "settings.llmModelQwen3_4B",
  t_lite_it21: "settings.llmModelTLiteIt21",
  qwen25_7b: "settings.llmModelQwen25_7B",
  gec08b: "settings.llmModelGec08B",
};

const LLM_MODEL_NAMES: Record<LlmModelKind, MessageKey> = {
  qwen3_4b: "settings.llmModelNameQwen3_4B",
  t_lite_it21: "settings.llmModelNameTLiteIt21",
  qwen25_7b: "settings.llmModelNameQwen25_7B",
  gec08b: "settings.llmModelNameGec08B",
};

const LLM_MODEL_SIZE_MB: Record<LlmModelKind, number> = {
  qwen3_4b: 2_500,
  t_lite_it21: 5_000,
  qwen25_7b: 4_700,
  gec08b: 500,
};

function aiRewriteAvailable(
  values: SettingsFormValues,
  diagnostics: DiagnosticsSnapshot | null,
  llmModels: LlmModelInfo[],
): boolean {
  if (values.text_rewrite_provider === "openai") {
    return values.has_api_key;
  }
  if (!diagnostics?.local_llm_compiled) {
    return false;
  }
  const selected = llmModels.find((model) => model.kind === values.local_llm_model);
  return selected?.exists ?? values.llm_model_exists;
}

function aiSkillEnabled(
  values: SettingsFormValues,
  diagnostics: DiagnosticsSnapshot | null,
  llmModels: LlmModelInfo[],
): boolean {
  return (
    aiRewriteAvailable(values, diagnostics, llmModels) &&
    values.text_processing_mode === "custom_skill"
  );
}

export function effectiveTranscriptionProvider(values: SettingsFormValues): string {
  if (!values.has_api_key && values.transcription_provider === "openai") {
    return "local";
  }
  return values.transcription_provider;
}

export function needsOpenAiApiKey(values: SettingsFormValues): boolean {
  return (
    values.transcription_provider === "openai" ||
    values.text_rewrite_provider === "openai"
  );
}

export const AI_TEXT_MODES = new Set<TextProcessingMode>(["optimization", "custom_skill"]);

function renderTextModeOptions(
  values: SettingsFormValues,
  diagnostics: DiagnosticsSnapshot | null,
  llmModels: LlmModelInfo[],
): string {
  const modes: Array<{ value: TextProcessingMode; label: MessageKey }> = [
    { value: "original", label: "settings.textModeOriginal" },
    { value: "basic", label: "settings.textModeBasic" },
    { value: "optimization", label: "settings.textModeOptimization" },
    { value: "custom_skill", label: "settings.textModeCustomSkill" },
  ];
  const aiAvailable = aiRewriteAvailable(values, diagnostics, llmModels);

  return modes
    .map(({ value, label }) => {
      const disabled = !aiAvailable && AI_TEXT_MODES.has(value);
      return `<option value="${value}" ${values.text_processing_mode === value ? "selected" : ""} ${disabled ? "disabled" : ""}>${escapeHtml(t(label))}</option>`;
    })
    .join("");
}

export function settingsToForm(
  settings: AppSettings,
  hasApiKey: boolean,
  whisperModelsDir = "",
  whisperModelExists = false,
  dictionaryPath = "",
  llmModelsDir = "",
  llmModelExists = false,
): SettingsFormValues {
  return {
    enabled: settings.enabled,
    global_hotkey: settings.global_hotkey,
    push_to_talk: settings.push_to_talk,
    ptt_hold: settings.ptt_hold ?? true,
    live_dictation_field_indicator: settings.live_dictation_field_indicator ?? true,
    hotkey_game_mode: settings.hotkey_game_mode ?? false,
    hotkey_block_system: settings.hotkey_block_system ?? false,
    microphone_device: settings.microphone_device ?? "",
    language: settings.language ?? "auto",
    injection_mode: settings.injection_mode,
    text_processing_mode: settings.text_processing_mode,
    spoken_punctuation: settings.spoken_punctuation,
    auto_punctuation_from_pauses: settings.auto_punctuation_from_pauses ?? true,
    numbers_as_words: settings.numbers_as_words,
    emulate_enter: settings.emulate_enter,
    enter_trigger_phrase: settings.enter_trigger_phrase,
    start_on_boot: settings.start_on_boot,
    capslock_ptt: settings.capslock_ptt ?? false,
    check_updates_on_startup: settings.check_updates_on_startup,
    show_notifications: settings.show_notifications,
    silence_timeout_ms: settings.silence_timeout_ms,
    ui_locale: settings.ui_locale,
    transcription_provider: settings.transcription_provider,
    local_whisper_model: settings.local_whisper_model ?? "base",
    local_whisper_models_dir: whisperModelsDir,
    whisper_model_exists: whisperModelExists,
    local_whisper_use_gpu: settings.local_whisper_use_gpu ?? false,
    local_whisper_beam_size: settings.local_whisper_beam_size ?? 1,
    text_rewrite_provider: settings.text_rewrite_provider ?? "openai",
    local_llm_model: settings.local_llm_model ?? "qwen3_4b",
    local_llm_models_dir: llmModelsDir,
    llm_model_exists: llmModelExists,
    local_llm_use_gpu: settings.local_llm_use_gpu ?? false,
    ai_rewrite_skill: settings.ai_rewrite_skill ?? "",
    whisper_prompt_prefix: settings.whisper_prompt_prefix ?? "",
    transcription_dictionary_path:
      settings.transcription_dictionary_path ?? dictionaryPath,
    audio_preprocess_enabled: settings.audio_preprocess_enabled ?? true,
    audio_noise_reduction_enabled: settings.audio_noise_reduction_enabled ?? true,
    vad_pre_speech_buffer_ms: settings.vad_pre_speech_buffer_ms ?? 300,
    vad_minimum_speech_ms: settings.vad_minimum_speech_ms ?? 250,
    vad_maximum_segment_ms: settings.vad_maximum_segment_ms ?? 30_000,
    vad_threshold_mode: settings.vad_threshold_mode ?? "auto",
    vad_voice_threshold_percent: settings.vad_voice_threshold_percent ?? 15,
    vad_auto_threshold_percent: settings.vad_auto_threshold_percent ?? 12,
    api_key: "",
    has_api_key: hasApiKey,
  };
}

export function renderTabBar(activeTab: SettingsTab): string {
  const tabs: Array<{ id: SettingsTab; label: string }> = [
    { id: "status", label: t("tabs.status") },
    { id: "voice", label: t("tabs.voice") },
    { id: "advanced", label: t("tabs.advanced") },
  ];

  return `
    <nav class="tab-bar" role="tablist">
      ${tabs
        .map(
          (tab) => `
            <button
              type="button"
              class="tab ${activeTab === tab.id ? "active" : ""}"
              data-tab="${tab.id}"
              role="tab"
              aria-selected="${activeTab === tab.id}"
            >
              ${escapeHtml(tab.label)}
            </button>
          `,
        )
        .join("")}
    </nav>
  `;
}

function formatDownloadProgress(progress: WhisperModelDownloadProgress): string {
  if (progress.percent !== null && progress.percent >= 0) {
    return t("settings.downloadingWhisper", { percent: Math.round(progress.percent) });
  }

  const downloadedMb = (progress.downloaded / (1024 * 1024)).toFixed(1);
  return t("settings.downloadingWhisperUnknown", { downloaded: downloadedMb });
}

function whisperDownloadPercent(
  progress: WhisperModelDownloadProgress | null,
): number | null {
  if (progress?.percent === null || progress?.percent === undefined) {
    return null;
  }
  return Math.max(0, Math.min(100, Math.round(progress.percent)));
}

export function updateWhisperDownloadUi(
  progress: WhisperModelDownloadProgress | null,
): void {
  const panel = document.querySelector<HTMLElement>("[data-whisper-download]");
  if (!panel || !progress) {
    return;
  }

  const percent = whisperDownloadPercent(progress);
  const hint = panel.querySelector<HTMLElement>("[data-whisper-download-hint]");
  if (hint) {
    hint.textContent = formatDownloadProgress(progress);
  }

  const bar = panel.querySelector<HTMLElement>("[data-whisper-download-bar]");
  const progressRoot = panel.querySelector<HTMLElement>(
    "[data-whisper-download-progress]",
  );
  if (!bar || !progressRoot) {
    return;
  }

  if (percent === null) {
    bar.style.width = "";
    bar.classList.add("is-indeterminate");
    progressRoot.setAttribute("aria-valuenow", "0");
    return;
  }

  bar.classList.remove("is-indeterminate");
  bar.style.width = `${percent}%`;
  progressRoot.setAttribute("aria-valuenow", String(percent));
}

function selectedWhisperModelExists(
  values: SettingsFormValues,
  whisperModels: WhisperModelInfo[],
): boolean {
  const selected = whisperModels.find((model) => model.kind === values.local_whisper_model);
  return selected?.exists ?? values.whisper_model_exists;
}

function renderWhisperModelAction(
  values: SettingsFormValues,
  whisperModels: WhisperModelInfo[],
  whisperModelDownload: WhisperModelDownloadProgress | null,
  downloadProgressPercent: number | null,
): string {
  const downloadingModel = whisperModelDownload !== null;
  const modelExists = selectedWhisperModelExists(values, whisperModels);

  if (downloadingModel) {
    return `
      <div class="whisper-download" data-whisper-download>
        <div class="field-row">
          <span class="field-hint" data-whisper-download-hint>${escapeHtml(formatDownloadProgress(whisperModelDownload))}</span>
        </div>
        <div
          class="download-progress"
          data-whisper-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${downloadProgressPercent ?? 0}"
        >
          <div
            class="download-progress-bar ${
              downloadProgressPercent === null ? "is-indeterminate" : ""
            }"
            data-whisper-download-bar
            style="${
              downloadProgressPercent === null ? "" : `width: ${downloadProgressPercent}%;`
            }"
          ></div>
        </div>
      </div>
    `;
  }

  if (modelExists) {
    const modelName = t(WHISPER_MODEL_NAMES[values.local_whisper_model]);
    return `<span class="field-hint">${escapeHtml(t("settings.whisperModelUsing", { model: modelName }))}</span>`;
  }

  return `
    <div class="field-row model-action">
      <button type="button" class="btn-secondary" data-download-whisper-model>
        ${escapeHtml(t("settings.downloadWhisperModel"))}
      </button>
    </div>
  `;
}

function renderWhisperModelOptions(values: SettingsFormValues, whisperModels: WhisperModelInfo[]): string {
  const kinds: WhisperModelKind[] = [
    "base",
    "small",
    "medium",
    "large_v3_turbo",
    "large_v3",
  ];
  return kinds
    .map((kind) => {
      const info = whisperModels.find((model) => model.kind === kind);
      const sizeMb = info?.size_mb ?? 0;
      const label = t(WHISPER_MODEL_LABELS[kind], { size: sizeMb });
      return `<option value="${kind}" ${values.local_whisper_model === kind ? "selected" : ""}>${escapeHtml(label)}</option>`;
    })
    .join("");
}

function llmDownloadPercent(progress: LlmModelDownloadProgress | null): number | null {
  return progress?.percent ?? null;
}

function selectedLlmModelExists(
  values: SettingsFormValues,
  llmModels: LlmModelInfo[],
): boolean {
  const selected = llmModels.find((model) => model.kind === values.local_llm_model);
  return selected?.exists ?? values.llm_model_exists;
}

function renderLlmModelAction(
  values: SettingsFormValues,
  llmModels: LlmModelInfo[],
  llmModelDownload: LlmModelDownloadProgress | null,
  downloadProgressPercent: number | null,
): string {
  const downloadingModel = llmModelDownload !== null;
  const modelExists = selectedLlmModelExists(values, llmModels);

  if (downloadingModel) {
    return `
      <div class="llm-download" data-llm-download>
        <div class="field-row">
          <span class="field-hint" data-llm-download-hint>${escapeHtml(formatDownloadProgress(llmModelDownload))}</span>
        </div>
        <div
          class="download-progress"
          data-llm-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${downloadProgressPercent ?? 0}"
        >
          <div
            class="download-progress-bar ${
              downloadProgressPercent === null ? "is-indeterminate" : ""
            }"
            data-llm-download-bar
            style="${
              downloadProgressPercent === null ? "" : `width: ${downloadProgressPercent}%;`
            }"
          ></div>
        </div>
      </div>
    `;
  }

  if (modelExists) {
    const modelName = t(LLM_MODEL_NAMES[values.local_llm_model]);
    return `<span class="field-hint">${escapeHtml(t("settings.llmModelUsing", { model: modelName }))}</span>`;
  }

  return `
    <div class="field-row model-action">
      <button type="button" class="btn-secondary" data-download-llm-model>
        ${escapeHtml(t("settings.downloadLlmModel"))}
      </button>
    </div>
  `;
}

function llmModelKindsForLocale(locale: UiLocale): LlmModelKind[] {
  const kinds: LlmModelKind[] = ["qwen3_4b", "t_lite_it21", "qwen25_7b", "gec08b"];
  return locale === "ru" ? kinds : kinds.filter((kind) => kind !== "t_lite_it21");
}

function renderLlmModelOptions(values: SettingsFormValues, llmModels: LlmModelInfo[]): string {
  const kinds = llmModelKindsForLocale(values.ui_locale);
  return kinds
    .map((kind) => {
      const info = llmModels.find((model) => model.kind === kind);
      const sizeMb = info?.size_mb ?? LLM_MODEL_SIZE_MB[kind];
      const label = t(LLM_MODEL_LABELS[kind], { size: sizeMb });
      return `<option value="${kind}" ${values.local_llm_model === kind ? "selected" : ""}>${escapeHtml(label)}</option>`;
    })
    .join("");
}

export function updateLlmDownloadUi(progress: LlmModelDownloadProgress | null): void {
  const panel = document.querySelector<HTMLElement>("[data-llm-download]");
  if (!panel) {
    return;
  }

  const percent = llmDownloadPercent(progress);
  const hint = panel.querySelector<HTMLElement>("[data-llm-download-hint]");
  if (hint) {
    hint.textContent = progress ? formatDownloadProgress(progress) : "";
  }

  const bar = panel.querySelector<HTMLElement>("[data-llm-download-bar]");
  const progressRoot = panel.querySelector<HTMLElement>("[data-llm-download-progress]");
  if (!bar || !progressRoot) {
    return;
  }

  if (percent === null) {
    bar.style.width = "";
    bar.classList.add("is-indeterminate");
    progressRoot.setAttribute("aria-valuenow", "0");
    return;
  }

  bar.classList.remove("is-indeterminate");
  bar.style.width = `${percent}%`;
  progressRoot.setAttribute("aria-valuenow", String(percent));
}

function renderAiSkillOptions(values: SettingsFormValues, aiSkills: AiSkillInfo[]): string {
  const options = aiSkills
    .map((skill) => {
      const label = skill.description.length > 0 ? `${skill.name} — ${skill.description}` : skill.name;
      return `<option value="${escapeHtml(skill.filename)}" ${values.ai_rewrite_skill === skill.filename ? "selected" : ""}>${escapeHtml(label)}</option>`;
    })
    .join("");

  if (values.text_processing_mode !== "custom_skill") {
    return `<option value="" selected>${escapeHtml(t("settings.aiSkillUnavailable"))}</option>`;
  }

  if (options.length === 0) {
    return `<option value="" selected>${escapeHtml(t("settings.aiSkillEmpty"))}</option>`;
  }

  return options;
}

function formatTranscriptionLanguageLabel(
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

function renderTranscriptionLanguageOptions(
  languages: TranscriptionLanguageInfo[],
  selected: string,
  uiLocale: UiLocale,
): string {
  const autoOption = `<option value="auto" ${selected === "auto" ? "selected" : ""}>${escapeHtml(t("settings.langAuto"))}</option>`;
  if (languages.length === 0) {
    return autoOption;
  }

  return (
    autoOption +
    languages
      .map((language) => {
        const label = formatTranscriptionLanguageLabel(
          language.code,
          language.name,
          uiLocale,
        );
        return `<option value="${escapeHtml(language.code)}" ${
          selected === language.code ? "selected" : ""
        }>${escapeHtml(label)}</option>`;
      })
      .join("")
  );
}

export function renderSettingsForm(
  values: SettingsFormValues,
  devices: string[],
  activeTab: SettingsTab,
  activityLog: ActivityLogEntry[],
  whisperModelDownload: WhisperModelDownloadProgress | null = null,
  whisperModels: WhisperModelInfo[] = [],
  aiSkills: AiSkillInfo[] = [],
  diagnostics: DiagnosticsSnapshot | null = null,
  llmModelDownload: LlmModelDownloadProgress | null = null,
  llmModels: LlmModelInfo[] = [],
  transcriptionLanguages: TranscriptionLanguageInfo[] = [],
): string {
  const downloadProgressPercent = whisperDownloadPercent(whisperModelDownload);
  const llmDownloadProgressPercent = llmDownloadPercent(llmModelDownload);
  const localLlmCompiled = diagnostics?.local_llm_compiled ?? false;
  const localLlmGpuCompiled = diagnostics?.local_llm_gpu_compiled ?? false;
  const defaultMicOption = `<option value="" ${
    values.microphone_device.length === 0 ? "selected" : ""
  }>${escapeHtml(t("settings.defaultMic"))}</option>`;
  const deviceOptions =
    devices.length === 0
      ? defaultMicOption
      : defaultMicOption +
        devices
          .map((device) => {
            const label = shortenDeviceLabel(device);
            return `<option value="${escapeHtml(device)}" title="${escapeHtml(device)}" ${
              values.microphone_device === device ? "selected" : ""
            }>${escapeHtml(label)}</option>`;
          })
          .join("");

  const textModeHint = t(textModeHintKey(values.text_processing_mode));

  return `
    <form id="settings-form" class="settings-form">
      <div class="tab-panel tab-panel--log ${activeTab === "status" ? "active" : ""}" data-panel="status">
        ${renderActivityLog(activityLog)}
      </div>

      <div class="tab-panel tab-panel--voice ${activeTab === "voice" ? "active" : ""}" data-panel="voice">
        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.capture"))}</h3>
        <div class="field-grid voice-ptt-row">
          <label class="field checkbox voice-ptt-toggle">
            <input name="push_to_talk" type="checkbox" ${values.push_to_talk ? "checked" : ""} />
            <span>${escapeHtml(t("settings.ptt"))}</span>
          </label>

          <label class="field ptt-only voice-hotkey-field" ${values.push_to_talk ? "" : "hidden"}>
            <div class="hotkey-input-wrap">
              <button
                type="button"
                class="hotkey-capture"
                data-hotkey-capture
                data-value="${escapeHtml(values.global_hotkey)}"
              >
                ${escapeHtml(values.global_hotkey)}
              </button>
              <input
                name="global_hotkey"
                type="hidden"
                value="${escapeHtml(values.global_hotkey)}"
              />
            </div>
          </label>

          <label class="field checkbox ptt-only voice-ptt-hold-toggle" ${values.push_to_talk ? "" : "hidden"}>
            <input name="ptt_hold" type="checkbox" ${values.ptt_hold ? "checked" : ""} />
            <span>${escapeHtml(t("settings.pttHold"))}</span>
          </label>

          <label class="field checkbox ptt-only voice-field-indicator-toggle" ${values.push_to_talk ? "" : "hidden"}>
            <input name="live_dictation_field_indicator" type="checkbox" ${values.live_dictation_field_indicator ? "checked" : ""} />
            <span>${escapeHtml(t("settings.liveDictationFieldIndicator"))}</span>
          </label>
          ${
            diagnostics?.hotkey_game_mode_supported
              ? `<label class="field checkbox voice-game-mode-toggle">
            <input name="hotkey_game_mode" type="checkbox" ${values.hotkey_game_mode ? "checked" : ""} />
            <span>${escapeHtml(t("settings.hotkeyGameMode"))}</span>
          </label>
          <label class="field checkbox voice-block-system-toggle" ${values.hotkey_game_mode ? "" : "hidden"}>
            <input name="hotkey_block_system" type="checkbox" ${values.hotkey_block_system ? "checked" : ""} />
            <span>${escapeHtml(t("settings.hotkeyBlockSystem"))}</span>
          </label>`
              : ""
          }
          ${
            diagnostics?.capslock_ptt_supported
              ? `<label class="field checkbox ptt-only voice-capslock-toggle" ${values.push_to_talk ? "" : "hidden"}>
            <input name="capslock_ptt" type="checkbox" ${values.capslock_ptt ? "checked" : ""} />
            <span class="voice-capslock-text">
              <span class="voice-capslock-label">${escapeHtml(t("settings.capsLockPtt"))}</span>
              <span class="field-hint">${escapeHtml(t("settings.capsLockPttHint"))}</span>
            </span>
          </label>`
              : ""
          }
        </div>

        ${renderVadThresholdPanel(
          values.vad_threshold_mode,
          values.vad_voice_threshold_percent,
          values.vad_auto_threshold_percent,
          values.push_to_talk,
        )}
        </section>

        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.microphone"))}</h3>
        <div class="field-grid voice-fields">
          <div class="field mic-device-field">
            <span>${escapeHtml(t("settings.microphone"))}</span>
            <select
              name="microphone_device"
              class="device-select"
              title="${escapeHtml(values.microphone_device || t("settings.defaultMic"))}"
            >${deviceOptions}</select>
            ${renderMicMeter(true)}
          </div>
        </div>
        </section>

        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.recognition"))}</h3>
        <div class="field-grid voice-stt-row">
          <label class="field">
            <span>${escapeHtml(t("settings.transcriptionLanguage"))}</span>
            <select name="language">
              ${renderTranscriptionLanguageOptions(
                transcriptionLanguages,
                values.language,
                values.ui_locale,
              )}
            </select>
          </label>
          <label class="field">
            <span>${escapeHtml(t("settings.transcriptionProvider"))}</span>
            <select name="transcription_provider">
              <option value="local" ${effectiveTranscriptionProvider(values) === "local" ? "selected" : ""}>${escapeHtml(t("settings.transcriptionProviderLocal"))}</option>
              <option value="openai" ${effectiveTranscriptionProvider(values) === "openai" ? "selected" : ""} ${values.has_api_key ? "" : "disabled"}>${escapeHtml(t("settings.transcriptionProviderOpenai"))}</option>
            </select>
          </label>
        </div>

        <div class="local-whisper-panel" data-local-model-panel ${effectiveTranscriptionProvider(values) === "local" ? "" : "hidden"}>
          <label class="field">
            <span>${escapeHtml(t("settings.whisperModelSelect"))}</span>
            <select name="local_whisper_model" data-whisper-model-select>
              ${renderWhisperModelOptions(values, whisperModels)}
            </select>
            ${
              whisperModelHintKey(values.local_whisper_model)
                ? `<span class="field-hint">${escapeHtml(t(whisperModelHintKey(values.local_whisper_model)!))}</span>`
                : ""
            }
          </label>

          <label class="field">
            <span>${escapeHtml(t("settings.whisperModelsDir"))}</span>
            <div class="api-key-row">
              <input
                name="local_whisper_models_dir"
                type="text"
                value="${escapeHtml(values.local_whisper_models_dir)}"
                placeholder="${escapeHtml(t("settings.whisperModelsDirPlaceholder"))}"
                readonly
              />
              <button
                type="button"
                class="icon-btn"
                data-pick-whisper-models-dir
                title="${escapeHtml(t("settings.whisperModelsDirPick"))}"
                aria-label="${escapeHtml(t("settings.whisperModelsDirPick"))}"
              >${iconFolder()}</button>
            </div>
          </label>

          ${renderWhisperModelAction(values, whisperModels, whisperModelDownload, downloadProgressPercent)}

          <label class="field checkbox">
            <input
              name="local_whisper_use_gpu"
              type="checkbox"
              ${values.local_whisper_use_gpu ? "checked" : ""}
              ${diagnostics?.whisper_gpu_compiled ? "" : "disabled"}
            />
            <span>${escapeHtml(t("settings.whisperUseGpu"))}</span>
            ${
              diagnostics?.whisper_gpu_compiled
                ? `<span class="field-hint">${escapeHtml(t("settings.whisperBackend", { backend: whisperBackendLabel(diagnostics.whisper_backend) }))}</span>`
                : `<span class="field-hint">${escapeHtml(t("settings.whisperGpuUnavailable"))}</span>`
            }
          </label>

          <label class="field">
            <span>${escapeHtml(t("settings.whisperBeamSize"))}</span>
            <select name="local_whisper_beam_size">
              <option value="1" ${values.local_whisper_beam_size === 1 ? "selected" : ""}>${escapeHtml(t("settings.whisperBeamGreedy"))}</option>
              <option value="3" ${values.local_whisper_beam_size === 3 ? "selected" : ""}>${escapeHtml(t("settings.whisperBeam3"))}</option>
              <option value="5" ${values.local_whisper_beam_size === 5 ? "selected" : ""}>${escapeHtml(t("settings.whisperBeam5"))}</option>
            </select>
          </label>
        </div>

        <label class="field advanced-span-2">
          <span>${escapeHtml(t("settings.whisperPromptPrefix"))}</span>
          <textarea
            name="whisper_prompt_prefix"
            rows="2"
            maxlength="400"
            placeholder="${escapeHtml(t("settings.whisperPromptPrefixPlaceholder"))}"
          >${escapeHtml(values.whisper_prompt_prefix)}</textarea>
        </label>

        <label class="field advanced-span-2">
          <span>${escapeHtml(t("settings.transcriptionDictionary"))}</span>
          <div class="api-key-row">
            <input
              name="transcription_dictionary_path"
              type="text"
              value="${escapeHtml(values.transcription_dictionary_path)}"
              placeholder="${escapeHtml(t("settings.transcriptionDictionaryPlaceholder"))}"
              readonly
            />
            <button
              type="button"
              class="icon-btn"
              data-open-dictionary-folder
              title="${escapeHtml(t("settings.transcriptionDictionaryOpen"))}"
              aria-label="${escapeHtml(t("settings.transcriptionDictionaryOpen"))}"
            >${iconFolder()}</button>
            <button
              type="button"
              class="icon-btn"
              data-pick-transcription-dictionary
              title="${escapeHtml(t("settings.transcriptionDictionaryPick"))}"
              aria-label="${escapeHtml(t("settings.transcriptionDictionaryPick"))}"
            >${iconImport()}</button>
          </div>
        </label>

        <label class="field checkbox">
          <input name="audio_preprocess_enabled" type="checkbox" ${values.audio_preprocess_enabled ? "checked" : ""} />
          <span>${escapeHtml(t("settings.audioPreprocess"))}</span>
        </label>

        <label class="field checkbox">
          <input name="audio_noise_reduction_enabled" type="checkbox" ${values.audio_noise_reduction_enabled ? "checked" : ""} ${values.audio_preprocess_enabled ? "" : "disabled"} />
          <span>${escapeHtml(t("settings.audioNoiseReduction"))}</span>
        </label>
        </section>
      </div>

      <div class="tab-panel tab-panel--advanced ${activeTab === "advanced" ? "active" : ""}" data-panel="advanced">
        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.textProcessing"))}</h3>
        <div class="advanced-grid">
          <label class="field advanced-span-2">
            <span>${escapeHtml(t("settings.textRewriteProvider"))}</span>
            <select name="text_rewrite_provider" data-text-rewrite-provider>
              <option value="openai" ${values.text_rewrite_provider === "openai" ? "selected" : ""}>${escapeHtml(t("settings.textRewriteProviderOpenai"))}</option>
              <option value="local" ${values.text_rewrite_provider === "local" ? "selected" : ""} ${localLlmCompiled ? "" : "disabled"}>${escapeHtml(t("settings.textRewriteProviderLocal"))}</option>
            </select>
          </label>

          <div class="local-llm-panel llm-only advanced-span-2" data-local-llm-panel ${values.text_rewrite_provider === "local" ? "" : "hidden"}>
            ${
              !localLlmCompiled
                ? `<span class="field-hint">${escapeHtml(t("settings.localLlmNotCompiled"))}</span>`
                : ""
            }
            <label class="field">
              <span>${escapeHtml(t("settings.llmModelSelect"))}</span>
              <select name="local_llm_model" data-llm-model-select>
                ${renderLlmModelOptions(values, llmModels)}
              </select>
            </label>

            <label class="field">
              <span>${escapeHtml(t("settings.llmModelsDir"))}</span>
              <div class="api-key-row">
                <input
                  name="local_llm_models_dir"
                  type="text"
                  value="${escapeHtml(values.local_llm_models_dir)}"
                  placeholder="${escapeHtml(t("settings.llmModelsDirPlaceholder"))}"
                  readonly
                />
                <button
                  type="button"
                  class="icon-btn"
                  data-pick-llm-models-dir
                  title="${escapeHtml(t("settings.llmModelsDirPick"))}"
                  aria-label="${escapeHtml(t("settings.llmModelsDirPick"))}"
                >${iconFolder()}</button>
              </div>
            </label>

            ${renderLlmModelAction(values, llmModels, llmModelDownload, llmDownloadProgressPercent)}

            <label class="field checkbox">
              <input name="local_llm_use_gpu" type="checkbox" ${values.local_llm_use_gpu ? "checked" : ""} ${localLlmGpuCompiled ? "" : "disabled"} />
              <span>${escapeHtml(t("settings.llmUseGpu"))}</span>
            </label>
          </div>

          <label class="field advanced-span-2">
            <span>${escapeHtml(t("settings.textMode"))}</span>
            <select name="text_processing_mode" data-text-mode-select>
              ${renderTextModeOptions(values, diagnostics, llmModels)}
            </select>
            <span class="field-hint" data-text-mode-hint>${escapeHtml(textModeHint)}</span>
          </label>

          <label class="field advanced-span-2 ai-skill-field" data-ai-skill-field ${aiSkillEnabled(values, diagnostics, llmModels) ? "" : "hidden"}>
            <span>${escapeHtml(t("settings.aiSkill"))}</span>
            <div class="ai-skill-row">
              <select name="ai_rewrite_skill" data-ai-skill-select ${aiSkillEnabled(values, diagnostics, llmModels) ? "" : "disabled"}>${renderAiSkillOptions(values, aiSkills)}</select>
              <div class="field-actions">
                <button type="button" class="icon-btn" data-open-skills-folder title="${escapeHtml(t("settings.aiSkillOpenFolder"))}" aria-label="${escapeHtml(t("settings.aiSkillOpenFolder"))}" ${aiSkillEnabled(values, diagnostics, llmModels) ? "" : "disabled"}>${iconFolder()}</button>
                <button type="button" class="icon-btn" data-import-skill title="${escapeHtml(t("settings.aiSkillImport"))}" aria-label="${escapeHtml(t("settings.aiSkillImport"))}" ${aiSkillEnabled(values, diagnostics, llmModels) ? "" : "disabled"}>${iconImport()}</button>
                <button type="button" class="icon-btn" data-open-skill-catalog title="${escapeHtml(t("settings.aiSkillBrowseCatalog"))}" aria-label="${escapeHtml(t("settings.aiSkillBrowseCatalog"))}" ${aiSkillEnabled(values, diagnostics, llmModels) ? "" : "disabled"}>${iconPlus()}</button>
              </div>
            </div>
          </label>
        </div>
        </section>

        <section class="settings-section" data-openai-connections-section ${needsOpenAiApiKey(values) ? "" : "hidden"}>
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.connections"))}</h3>
        <div class="advanced-grid">
          <label class="field advanced-span-2" data-openai-api-key-panel ${needsOpenAiApiKey(values) ? "" : "hidden"}>
            <span>${escapeHtml(t("settings.apiKey"))}</span>
            <div class="api-key-row">
              <input
                name="api_key"
                type="password"
                placeholder="${values.has_api_key ? escapeHtml(t("settings.apiKeyConfigured")) : escapeHtml(t("settings.apiKeyPlaceholder"))}"
                autocomplete="off"
              />
              ${
                values.has_api_key
                  ? `<button type="button" class="icon-btn" data-clear-api-key title="${escapeHtml(t("settings.apiKeyClear"))}" aria-label="${escapeHtml(t("settings.apiKeyClear"))}">${iconTrash()}</button>`
                  : ""
              }
            </div>
          </label>
        </div>
        </section>

        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.system"))}</h3>
        <div class="advanced-grid">
          <label class="field">
            <span>${escapeHtml(t("settings.uiLocale"))}</span>
            <select name="ui_locale">
              <option value="en" ${values.ui_locale === "en" ? "selected" : ""}>${escapeHtml(t("settings.uiLocaleEn"))}</option>
              <option value="ru" ${values.ui_locale === "ru" ? "selected" : ""}>${escapeHtml(t("settings.uiLocaleRu"))}</option>
            </select>
          </label>

          <label class="field">
            <span>${escapeHtml(t("settings.injection"))}</span>
            <select name="injection_mode">
              <option value="auto" ${values.injection_mode === "auto" ? "selected" : ""}>${escapeHtml(t("settings.injectionAuto"))}</option>
              <option value="paste" ${values.injection_mode === "paste" ? "selected" : ""}>${escapeHtml(t("settings.injectionPaste"))}</option>
              <option value="keyboard" ${values.injection_mode === "keyboard" ? "selected" : ""}>${escapeHtml(t("settings.injectionKeyboard"))}</option>
            </select>
          </label>

          <details class="vad-details advanced-span-2">
            <summary>${escapeHtml(t("settings.vadAdvanced"))}</summary>
            <div class="advanced-grid vad-grid">
              <label class="field">
                <span>${escapeHtml(t("settings.silenceTimeout"))}</span>
                <input
                  name="silence_timeout_ms"
                  type="number"
                  min="200"
                  max="10000"
                  step="50"
                  value="${values.silence_timeout_ms}"
                />
              </label>
              <label class="field">
                <span>${escapeHtml(t("settings.vadPreSpeech"))}</span>
                <input
                  name="vad_pre_speech_buffer_ms"
                  type="number"
                  min="50"
                  max="2000"
                  step="50"
                  value="${values.vad_pre_speech_buffer_ms}"
                />
              </label>
              <label class="field">
                <span>${escapeHtml(t("settings.vadMinimumSpeech"))}</span>
                <input
                  name="vad_minimum_speech_ms"
                  type="number"
                  min="50"
                  max="2000"
                  step="50"
                  value="${values.vad_minimum_speech_ms}"
                />
              </label>
              <label class="field">
                <span>${escapeHtml(t("settings.vadMaximumSegment"))}</span>
                <input
                  name="vad_maximum_segment_ms"
                  type="number"
                  min="1000"
                  max="120000"
                  step="1000"
                  value="${values.vad_maximum_segment_ms}"
                />
              </label>
            </div>
          </details>
        </div>

        <div class="toggle-grid advanced-toggles">
          <label class="field checkbox">
            <input name="spoken_punctuation" type="checkbox" ${values.spoken_punctuation ? "checked" : ""} />
            <span>${escapeHtml(t("settings.spokenPunctuation"))}</span>
          </label>

          <label class="field checkbox" data-local-only-toggle>
            <input name="auto_punctuation_from_pauses" type="checkbox" ${values.auto_punctuation_from_pauses ? "checked" : ""} />
            <span>${escapeHtml(t("settings.autoPunctuationFromPauses"))}</span>
          </label>

          <label class="field checkbox">
            <input name="numbers_as_words" type="checkbox" ${values.numbers_as_words ? "checked" : ""} />
            <span>${escapeHtml(t("settings.numbersAsWords"))}</span>
          </label>

          <label class="field checkbox enter-emulation-check">
            <input name="emulate_enter" type="checkbox" ${values.emulate_enter ? "checked" : ""} />
            <span>${escapeHtml(t("settings.emulateEnter"))}</span>
          </label>

          <label class="field enter-phrase-only" ${values.emulate_enter ? "" : "hidden"}>
            <input
              name="enter_trigger_phrase"
              type="text"
              value="${escapeHtml(values.enter_trigger_phrase)}"
              placeholder="${escapeHtml(t("settings.enterTriggerPhrasePlaceholder"))}"
              aria-label="${escapeHtml(t("settings.enterTriggerPhrase"))}"
              maxlength="120"
            />
          </label>

          <label class="field checkbox">
            <input name="start_on_boot" type="checkbox" ${values.start_on_boot ? "checked" : ""} />
            <span>${escapeHtml(t("settings.startOnBoot"))}</span>
          </label>

          <label class="field checkbox">
            <input name="check_updates_on_startup" type="checkbox" ${values.check_updates_on_startup ? "checked" : ""} />
            <span>${escapeHtml(t("settings.checkUpdatesOnStartup"))}</span>
          </label>

          <label class="field checkbox">
            <input name="show_notifications" type="checkbox" ${values.show_notifications ? "checked" : ""} />
            <span>${escapeHtml(t("settings.notifications"))}</span>
          </label>
        </div>
        </section>
      </div>
    </form>
  `;
}

export function readSettingsForm(form: HTMLFormElement): SettingsFormValues {
  const data = new FormData(form);
  return {
    enabled: false,
    global_hotkey: String(data.get("global_hotkey") ?? ""),
    push_to_talk: data.get("push_to_talk") === "on",
    ptt_hold: data.get("ptt_hold") === "on",
    live_dictation_field_indicator: data.get("live_dictation_field_indicator") === "on",
    hotkey_game_mode: data.get("hotkey_game_mode") === "on",
    hotkey_block_system: data.get("hotkey_block_system") === "on",
    microphone_device: String(data.get("microphone_device") ?? ""),
    language: String(data.get("language") ?? "auto"),
    injection_mode: String(data.get("injection_mode") ?? "auto") as InjectionMode,
    text_processing_mode: String(
      data.get("text_processing_mode") ?? "basic",
    ) as TextProcessingMode,
    spoken_punctuation: data.get("spoken_punctuation") === "on",
    auto_punctuation_from_pauses: data.get("auto_punctuation_from_pauses") === "on",
    numbers_as_words: data.get("numbers_as_words") === "on",
    emulate_enter: data.get("emulate_enter") === "on",
    enter_trigger_phrase: String(data.get("enter_trigger_phrase") ?? ""),
    start_on_boot: data.get("start_on_boot") === "on",
    check_updates_on_startup: data.get("check_updates_on_startup") === "on",
    capslock_ptt: data.get("capslock_ptt") === "on",
    show_notifications: data.get("show_notifications") === "on",
    silence_timeout_ms: Number(data.get("silence_timeout_ms") ?? 700),
    ui_locale: String(data.get("ui_locale") ?? "en") as UiLocale,
    transcription_provider: String(data.get("transcription_provider") ?? "local"),
    local_whisper_model: String(data.get("local_whisper_model") ?? "base") as WhisperModelKind,
    local_whisper_models_dir: String(data.get("local_whisper_models_dir") ?? ""),
    whisper_model_exists: false,
    local_whisper_use_gpu: data.get("local_whisper_use_gpu") === "on",
    local_whisper_beam_size: Number(data.get("local_whisper_beam_size") ?? 1),
    text_rewrite_provider: String(
      data.get("text_rewrite_provider") ?? "openai",
    ) as TextRewriteProvider,
    local_llm_model: String(data.get("local_llm_model") ?? "qwen3_4b") as LlmModelKind,
    local_llm_models_dir: String(data.get("local_llm_models_dir") ?? ""),
    llm_model_exists: false,
    local_llm_use_gpu: data.get("local_llm_use_gpu") === "on",
    ai_rewrite_skill: String(data.get("ai_rewrite_skill") ?? ""),
    whisper_prompt_prefix: String(data.get("whisper_prompt_prefix") ?? ""),
    transcription_dictionary_path: String(data.get("transcription_dictionary_path") ?? ""),
    audio_preprocess_enabled: data.get("audio_preprocess_enabled") === "on",
    audio_noise_reduction_enabled: data.get("audio_noise_reduction_enabled") === "on",
    vad_pre_speech_buffer_ms: Number(data.get("vad_pre_speech_buffer_ms") ?? 300),
    vad_minimum_speech_ms: Number(data.get("vad_minimum_speech_ms") ?? 250),
    vad_maximum_segment_ms: Number(data.get("vad_maximum_segment_ms") ?? 30_000),
    vad_threshold_mode: (String(data.get("vad_threshold_mode") ?? "auto") === "manual"
      ? "manual"
      : "auto") as VadThresholdMode,
    vad_voice_threshold_percent: Number(data.get("vad_voice_threshold_percent") ?? 15),
    vad_auto_threshold_percent: Number(
      data.get("vad_auto_threshold_percent") ?? 12,
    ),
    api_key: String(data.get("api_key") ?? ""),
    has_api_key: false,
  };
}

export function textModeHintKey(mode: TextProcessingMode): MessageKey {
  return TEXT_MODE_HINT_KEYS[mode];
}

const DEVICE_LABEL_MAX = 34;

function shortenDeviceLabel(value: string): string {
  if (value.length <= DEVICE_LABEL_MAX) {
    return value;
  }
  return `${value.slice(0, DEVICE_LABEL_MAX - 1)}…`;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
