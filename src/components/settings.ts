import {
  effectiveLocalSttFamily,
  effectiveLocalSttQuant,
  isLocalWhisperBeamSizeActive,
  type ActivityLogEntry,
  type AiSkillInfo,
  type AppSettings,
  type DiagnosticsSnapshot,
  type InjectionMode,
  type LlmModelDownloadProgress,
  type LlmModelInfo,
  type LlmModelKind,
  type LocalSttFamily,
  type LocalSttFamilyInfo,
  type LocalSttModelKind,
  type LocalSttQuant,
  type LocalSttVariantInfo,
  type TextProcessingMode,
  type TextRewriteProvider,
  type TranscriptionLanguageInfo,
  type UiLocale,
  type SileroModelDownloadProgress,
  type SileroTeModelStatus,
  type SileroVadModelStatus,
  type WhisperModelDownloadProgress,
} from "../api";
import { readLocalSttQuantFromForm, renderSttModelPicker } from "./stt-model-picker";
import { getState, type SettingsTab } from "../state";
import { t } from "../i18n";
import type { MessageKey } from "../i18n/locales/en";
import { iconFolder, iconImport, iconPlus, iconTrash } from "./icons";
import { renderMicMeter } from "./mic-meter";
import { renderStatusDashboard } from "./status-dashboard";
import { renderVadThresholdPanel } from "./vad-threshold-panel";
import { isStatusQuickSettingsOpen } from "./status-quick-settings";
import { renderStatusEventsPopover } from "./status-events";
import type { VadEngine, VadThresholdMode } from "../api";

export interface SettingsFormValues {
  enabled: boolean;
  global_hotkey: string;
  push_to_talk: boolean;
  ptt_hold: boolean;
  recording_indicator: boolean;
  abort_on_focus_loss: boolean;
  microphone_device: string;
  language: string;
  injection_mode: InjectionMode;
  text_processing_mode: TextProcessingMode;
  spoken_punctuation: boolean;
  silero_te: boolean;
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
  local_stt_model: LocalSttModelKind;
  local_stt_family: LocalSttFamily;
  local_stt_quant: LocalSttQuant;
  whisper_model_exists: boolean;
  local_whisper_use_gpu: boolean;
  local_whisper_beam_size: number;
  text_rewrite_provider: TextRewriteProvider;
  local_llm_model: LlmModelKind;
  llm_model_exists: boolean;
  local_llm_use_gpu: boolean;
  ai_rewrite_skill: string;
  whisper_prompt_prefix: string;
  data_storage_dir: string;
  audio_preprocess_enabled: boolean;
  audio_noise_reduction_enabled: boolean;
  vad_pre_speech_buffer_ms: number;
  vad_minimum_speech_ms: number;
  vad_maximum_segment_ms: number;
  vad_engine: VadEngine;
  vad_threshold_mode: VadThresholdMode;
  vad_voice_threshold_percent: number;
  vad_auto_threshold_percent: number;
  stt_idle_unload_sec: number;
  llm_idle_unload_sec: number;
  prewarm_local_models_at_startup: boolean;
  weak_pc_mode: boolean;
  weak_pc_spill_to_disk: boolean;
  weak_pc_ram_segment_cap: number;
  weak_pc_max_disk_queue_mb: number;
  weak_pc_reduce_prewarm: boolean;
  api_key: string;
  has_api_key: boolean;
}

const TEXT_MODE_HINT_KEYS: Record<TextProcessingMode, MessageKey> = {
  original: "settings.textModeHintOriginal",
  basic: "settings.textModeHintBasic",
  optimization: "settings.textModeHintOptimization",
  custom_skill: "settings.textModeHintCustomSkill",
};

type LlmModelOptionCopy = {
  name: MessageKey;
  req: MessageKey;
  features: MessageKey;
};

const LLM_MODEL_OPTION_COPY: Record<LlmModelKind, LlmModelOptionCopy> = {
  qwen3_4b: {
    name: "settings.llmModelNameQwen3_4B",
    req: "settings.llmModelQwen3_4BReq",
    features: "settings.llmModelQwen3_4BFeatures",
  },
  t_lite_it21: {
    name: "settings.llmModelNameTLiteIt21",
    req: "settings.llmModelTLiteIt21Req",
    features: "settings.llmModelTLiteIt21Features",
  },
  qwen25_7b: {
    name: "settings.llmModelNameQwen25_7B",
    req: "settings.llmModelQwen25_7BReq",
    features: "settings.llmModelQwen25_7BFeatures",
  },
  gec08b: {
    name: "settings.llmModelNameGec08B",
    req: "settings.llmModelGec08BReq",
    features: "settings.llmModelGec08BFeatures",
  },
};

const LLM_MODEL_SIZE_MB: Record<LlmModelKind, number> = {
  qwen3_4b: 2_500,
  t_lite_it21: 5_000,
  qwen25_7b: 4_700,
  gec08b: 500,
};

function formatSttModelSize(sizeMb: number): string {
  if (sizeMb <= 0) {
    return "…";
  }
  return t("settings.sttModelSizeFormat", { size: sizeMb });
}

function formatModelOptionLabel(
  nameKey: MessageKey,
  sizeMb: number,
  reqKey: MessageKey,
  featuresKey: MessageKey,
): string {
  const title = `${t(nameKey)} (${formatSttModelSize(sizeMb)})`;
  return `${title}\n${t(reqKey)}\n${t(featuresKey)}`;
}

function formatLlmModelOptionLabel(kind: LlmModelKind, sizeMb: number): string {
  const copy = LLM_MODEL_OPTION_COPY[kind];
  return formatModelOptionLabel(copy.name, sizeMb, copy.req, copy.features);
}

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

export function needsOpenAiApiKeyForProviders(
  transcriptionProvider: string,
  textRewriteProvider: TextRewriteProvider,
  hasApiKey: boolean,
): boolean {
  const effective =
    !hasApiKey && transcriptionProvider === "openai" ? "local" : transcriptionProvider;
  return effective === "openai" || textRewriteProvider === "openai";
}

export function needsOpenAiApiKey(values: SettingsFormValues): boolean {
  return needsOpenAiApiKeyForProviders(
    values.transcription_provider,
    values.text_rewrite_provider,
    values.has_api_key,
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
  const pttEnabled = values.push_to_talk;

  return modes
    .map(({ value, label }) => {
      const disabled =
        AI_TEXT_MODES.has(value) && (!aiAvailable || !pttEnabled);
      return `<option value="${value}" ${values.text_processing_mode === value ? "selected" : ""} ${disabled ? "disabled" : ""}>${escapeHtml(t(label))}</option>`;
    })
    .join("");
}

export function settingsToForm(
  settings: AppSettings,
  hasApiKey: boolean,
  dataStorageDir = "",
  whisperModelExists = false,
  llmModelExists = false,
): SettingsFormValues {
  const sttFamily = effectiveLocalSttFamily(settings);
  const sttQuant = effectiveLocalSttQuant(sttFamily, settings.local_stt_quant);
  return {
    enabled: settings.enabled,
    global_hotkey: settings.global_hotkey,
    push_to_talk: settings.push_to_talk,
    ptt_hold: settings.ptt_hold ?? true,
    recording_indicator:
      settings.recording_indicator ??
      settings.live_dictation_field_indicator ??
      true,
    abort_on_focus_loss: settings.abort_on_focus_loss ?? true,
    microphone_device: settings.microphone_device ?? "",
    language: settings.language ?? "auto",
    injection_mode: settings.injection_mode,
    text_processing_mode: settings.text_processing_mode,
    spoken_punctuation: settings.spoken_punctuation,
    silero_te: settings.silero_te ?? true,
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
    local_stt_model:
      settings.local_stt_model ??
      (settings as { local_whisper_model?: LocalSttModelKind }).local_whisper_model ??
      "base",
    local_stt_family: sttFamily,
    local_stt_quant: sttQuant,
    whisper_model_exists: whisperModelExists,
    local_whisper_use_gpu: settings.local_whisper_use_gpu ?? false,
    local_whisper_beam_size: settings.local_whisper_beam_size ?? 1,
    text_rewrite_provider: settings.text_rewrite_provider ?? "openai",
    local_llm_model: settings.local_llm_model ?? "qwen3_4b",
    llm_model_exists: llmModelExists,
    local_llm_use_gpu: settings.local_llm_use_gpu ?? false,
    ai_rewrite_skill: settings.ai_rewrite_skill ?? "",
    whisper_prompt_prefix: settings.whisper_prompt_prefix ?? "",
    data_storage_dir: dataStorageDir.trim(),
    audio_preprocess_enabled: settings.audio_preprocess_enabled ?? true,
    audio_noise_reduction_enabled: settings.audio_noise_reduction_enabled ?? true,
    vad_pre_speech_buffer_ms: settings.vad_pre_speech_buffer_ms ?? 300,
    vad_minimum_speech_ms: settings.vad_minimum_speech_ms ?? 250,
    vad_maximum_segment_ms: settings.vad_maximum_segment_ms ?? 30_000,
    vad_engine: settings.vad_engine ?? "silero",
    vad_threshold_mode: settings.vad_threshold_mode ?? "auto",
    vad_voice_threshold_percent: settings.vad_voice_threshold_percent ?? 15,
    vad_auto_threshold_percent: settings.vad_auto_threshold_percent ?? 12,
    stt_idle_unload_sec: settings.stt_idle_unload_sec ?? 180,
    llm_idle_unload_sec: settings.llm_idle_unload_sec ?? 90,
    prewarm_local_models_at_startup: settings.prewarm_local_models_at_startup ?? false,
    weak_pc_mode: settings.weak_pc_mode ?? false,
    weak_pc_spill_to_disk: settings.weak_pc_spill_to_disk ?? true,
    weak_pc_ram_segment_cap: settings.weak_pc_ram_segment_cap ?? 2,
    weak_pc_max_disk_queue_mb: settings.weak_pc_max_disk_queue_mb ?? 512,
    weak_pc_reduce_prewarm: settings.weak_pc_reduce_prewarm ?? true,
    api_key: "",
    has_api_key: hasApiKey,
  };
}

export function syncSettingsTabUi(activeTab: SettingsTab): void {
  document.querySelectorAll<HTMLButtonElement>(".tab[data-tab]").forEach((button) => {
    const selected = button.dataset.tab === activeTab;
    button.classList.toggle("active", selected);
    button.setAttribute("aria-selected", selected ? "true" : "false");
    button.tabIndex = selected ? 0 : -1;
  });
  document.querySelectorAll<HTMLElement>(".tab-panel[data-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.panel === activeTab);
  });
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

function llmDownloadPercent(progress: LlmModelDownloadProgress | null): number | null {
  return progress?.percent ?? null;
}

function renderSileroTeModelAction(
  values: SettingsFormValues,
  sileroTeCompiled: boolean,
  sileroTeReady: boolean,
  sileroTeDownload: SileroModelDownloadProgress | null,
  downloadProgressPercent: number | null,
): string {
  if (effectiveTranscriptionProvider(values) !== "local") {
    return "";
  }
  if (!sileroTeCompiled) {
    return "";
  }
  if (sileroTeReady && !sileroTeDownload) {
    return "";
  }

  if (sileroTeDownload) {
    return `
      <div class="silero-te-download" data-silero-te-download>
        <div class="field-row">
          <span class="field-hint" data-silero-te-download-hint>${escapeHtml(formatDownloadProgress(sileroTeDownload))}</span>
        </div>
        <div
          class="download-progress"
          data-silero-te-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${downloadProgressPercent ?? 0}"
        >
          <div
            class="download-progress-bar ${
              downloadProgressPercent === null ? "is-indeterminate" : ""
            }"
            data-silero-te-download-bar
            style="${downloadProgressPercent === null ? "" : `width: ${downloadProgressPercent}%;`}"
          ></div>
        </div>
      </div>
    `;
  }

  return `
    <div class="field-row model-action" data-silero-te-download-action>
      <button type="button" class="btn-secondary" data-download-silero-te-model>
        ${escapeHtml(t("settings.downloadWhisperModel"))}
      </button>
    </div>
  `;
}

export function ensureSileroTeDownloadUi(
  progress: SileroModelDownloadProgress,
): void {
  if (!document.querySelector("[data-silero-te-download]")) {
    const action = document.querySelector("[data-silero-te-download-action]");
    if (action) {
      const percent = whisperDownloadPercent(progress);
      action.outerHTML = `
      <div class="silero-te-download" data-silero-te-download>
        <div class="field-row">
          <span class="field-hint" data-silero-te-download-hint">${escapeHtml(formatDownloadProgress(progress))}</span>
        </div>
        <div
          class="download-progress"
          data-silero-te-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${percent ?? 0}"
        >
          <div
            class="download-progress-bar ${percent === null ? "is-indeterminate" : ""}"
            data-silero-te-download-bar
            style="${percent === null ? "" : `width: ${percent}%;`}"
          ></div>
        </div>
      </div>`;
    }
  }
  updateSileroTeDownloadUi(progress);
}

export function updateSileroTeDownloadUi(
  progress: SileroModelDownloadProgress | null,
): void {
  const panel = document.querySelector<HTMLElement>("[data-silero-te-download]");
  if (!panel || !progress) {
    return;
  }

  const percent = whisperDownloadPercent(progress);
  const hint = panel.querySelector<HTMLElement>("[data-silero-te-download-hint]");
  if (hint) {
    hint.textContent = formatDownloadProgress(progress);
  }

  const bar = panel.querySelector<HTMLElement>("[data-silero-te-download-bar]");
  const progressRoot = panel.querySelector<HTMLElement>("[data-silero-te-download-progress]");
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
    return "";
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
      const label = formatLlmModelOptionLabel(kind, sizeMb);
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

export function renderStatusQuickSettingsPopover(
  values: SettingsFormValues,
  systemNotificationsAvailable: boolean,
): string {
  const panelOpen = isStatusQuickSettingsOpen();
  return `
    <div class="status-quick-settings" data-status-quick-settings-popover ${panelOpen ? "" : "hidden"}>
      <div
        class="status-quick-settings-panel"
        id="status-quick-settings-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="status-quick-settings-title"
      >
        <header class="status-quick-settings-header">
          <h4 class="status-quick-settings-title" id="status-quick-settings-title">
            ${escapeHtml(t("status.quickSettingsTitle"))}
          </h4>
          <button
            type="button"
            class="status-quick-settings-close icon-btn"
            data-close-status-quick-settings
            aria-label="${escapeHtml(t("common.close"))}"
            title="${escapeHtml(t("common.close"))}"
          >×</button>
        </header>
        <div class="status-quick-settings-body">
          <label class="field">
            <span>${escapeHtml(t("settings.uiLocale"))}</span>
            <select name="ui_locale">
              <option value="en" ${values.ui_locale === "en" ? "selected" : ""}>${escapeHtml(t("settings.uiLocaleEn"))}</option>
              <option value="ru" ${values.ui_locale === "ru" ? "selected" : ""}>${escapeHtml(t("settings.uiLocaleRu"))}</option>
            </select>
          </label>

          <label class="field">
            <span>${escapeHtml(t("settings.dataStorage"))}</span>
            <button type="button" class="btn-secondary" data-pick-data-storage-dir>
              ${escapeHtml(t("settings.dataStoragePick"))}
            </button>
          </label>

          ${renderWeakPcExpertSettings({
            weak_pc_mode: values.weak_pc_mode,
            weak_pc_spill_to_disk: values.weak_pc_spill_to_disk,
            weak_pc_ram_segment_cap: values.weak_pc_ram_segment_cap,
            weak_pc_max_disk_queue_mb: values.weak_pc_max_disk_queue_mb,
            weak_pc_reduce_prewarm: values.weak_pc_reduce_prewarm,
          })}

          <div class="status-quick-settings-subsection">
            <h5 class="status-quick-settings-subtitle">${escapeHtml(t("settings.memoryAdvanced"))}</h5>
            <div class="status-quick-settings-grid">
              <label class="field checkbox status-quick-settings-span">
                <input
                  name="prewarm_local_models_at_startup"
                  type="checkbox"
                  ${values.prewarm_local_models_at_startup ? "checked" : ""}
                />
                <span>${escapeHtml(t("settings.prewarmLocalModelsAtStartup"))}</span>
              </label>
              ${renderIdleUnloadSlider("settings.sttIdleUnload", "stt_idle_unload_sec", values.stt_idle_unload_sec)}
              ${renderIdleUnloadSlider("settings.llmIdleUnload", "llm_idle_unload_sec", values.llm_idle_unload_sec)}
              <p class="field-hint status-quick-settings-span">${escapeHtml(t("settings.memoryIdleHint"))}</p>
            </div>
          </div>

          <div class="status-quick-settings-toggles">
            <label class="field checkbox">
              <input name="start_on_boot" type="checkbox" ${values.start_on_boot ? "checked" : ""} />
              <span>${escapeHtml(t("settings.startOnBoot"))}</span>
            </label>
            <label class="field checkbox">
              <input name="check_updates_on_startup" type="checkbox" ${values.check_updates_on_startup ? "checked" : ""} />
              <span>${escapeHtml(t("settings.checkUpdatesOnStartup"))}</span>
            </label>
            <label class="field checkbox">
              <input
                name="show_notifications"
                type="checkbox"
                ${values.show_notifications && systemNotificationsAvailable ? "checked" : ""}
                ${systemNotificationsAvailable ? "" : "disabled"}
              />
              <span>${escapeHtml(t("settings.notifications"))}</span>
            </label>
          </div>
        </div>
      </div>
    </div>`;
}

export function renderSettingsForm(
  values: SettingsFormValues,
  devices: string[],
  activeTab: SettingsTab,
  activityLog: ActivityLogEntry[],
  whisperModelDownload: WhisperModelDownloadProgress | null = null,
  localSttFamilies: LocalSttFamilyInfo[] = [],
  sttVariantInfo: LocalSttVariantInfo | null = null,
  aiSkills: AiSkillInfo[] = [],
  diagnostics: DiagnosticsSnapshot | null = null,
  llmModelDownload: LlmModelDownloadProgress | null = null,
  llmModels: LlmModelInfo[] = [],
  transcriptionLanguages: TranscriptionLanguageInfo[] = [],
  sileroTeModel: SileroTeModelStatus | null = null,
  sileroVadModel: SileroVadModelStatus | null = null,
  sileroTeModelDownload: SileroModelDownloadProgress | null = null,
  sileroVadModelDownload: SileroModelDownloadProgress | null = null,
): string {
  const downloadProgressPercent = whisperDownloadPercent(whisperModelDownload);
  const llmDownloadProgressPercent = llmDownloadPercent(llmModelDownload);
  const sileroTeDownloadProgressPercent = whisperDownloadPercent(sileroTeModelDownload);
  const sileroVadDownloadProgressPercent = whisperDownloadPercent(sileroVadModelDownload);
  const sileroVadOnDisk = sileroVadModel?.exists === true;
  const sileroVadReady =
    sileroVadOnDisk || diagnostics?.vad_silero_runtime_ok === true;
  const sileroTeReady = sileroTeModel?.exists === true;
  const sileroTeCompiled =
    diagnostics?.silero_te_compiled === true || (sileroTeModel?.size_mb ?? 0) > 0;
  const sttVariantReady = sttVariantInfo?.exists === true || values.whisper_model_exists;
  const sileroCompiled = diagnostics?.vad_silero_compiled !== false;
  const localLlmCompiled = diagnostics?.local_llm_compiled ?? false;
  const localLlmGpuCompiled = diagnostics?.local_llm_gpu_compiled ?? false;
  const localSttGpuCompiled =
    diagnostics?.local_stt_gpu_compiled === true ||
    diagnostics?.whisper_gpu_compiled === true ||
    diagnostics?.sherpa_gpu_compiled === true;
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
  const systemNotificationsAvailable = diagnostics?.system_notifications_available ?? true;
  const showWhisperBeam = isLocalWhisperBeamSizeActive(values, {
    whisperCompiled: diagnostics?.whisper_local_compiled !== false,
  });

  return `
    <form id="settings-form" class="settings-form">
      ${renderStatusQuickSettingsPopover(values, systemNotificationsAvailable)}
      ${renderStatusEventsPopover(activityLog)}
      <div class="tab-panel tab-panel--status ${activeTab === "status" ? "active" : ""}" data-panel="status">
        ${renderStatusDashboard(values, diagnostics)}
      </div>

      <div class="tab-panel tab-panel--voice ${activeTab === "voice" ? "active" : ""}" data-panel="voice">
        <section class="settings-section settings-section--activation">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.activation"))}</h3>
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

          <label class="field checkbox voice-recording-indicator-toggle">
            <input name="recording_indicator" type="checkbox" ${values.recording_indicator ? "checked" : ""} />
            <span>${escapeHtml(t("settings.recordingIndicator"))}</span>
          </label>

          <label class="field checkbox voice-abort-focus-loss-toggle">
            <input name="abort_on_focus_loss" type="checkbox" ${values.abort_on_focus_loss ? "checked" : ""} />
            <span>${escapeHtml(t("settings.abortOnFocusLoss"))}</span>
          </label>
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
        </section>

        <section class="settings-section settings-section--capture-device">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.captureDevice"))}</h3>
        <div class="field-grid voice-fields">
          <div class="field mic-device-field">
            <select
              name="microphone_device"
              class="device-select"
              aria-label="${escapeHtml(t("settings.microphone"))}"
              title="${escapeHtml(values.microphone_device || t("settings.defaultMic"))}"
            >${deviceOptions}</select>
            ${renderMicMeter(true)}
          </div>
        </div>

        <label class="field checkbox">
          <input name="audio_preprocess_enabled" type="checkbox" ${values.audio_preprocess_enabled ? "checked" : ""} />
          <span>${escapeHtml(t("settings.audioPreprocess"))}</span>
        </label>

        <label class="field checkbox">
          <input name="audio_noise_reduction_enabled" type="checkbox" ${values.audio_noise_reduction_enabled ? "checked" : ""} ${values.audio_preprocess_enabled ? "" : "disabled"} />
          <span>${escapeHtml(t("settings.audioNoiseReduction"))}</span>
        </label>

        ${renderVadThresholdPanel(
          values.vad_threshold_mode,
          values.vad_voice_threshold_percent,
          values.vad_auto_threshold_percent,
          values.vad_engine,
          sileroCompiled && (values.vad_engine !== "silero" || sileroVadReady),
          diagnostics?.vad_engine,
          sileroCompiled,
          sileroVadOnDisk,
          sileroVadModelDownload,
          sileroVadDownloadProgressPercent,
        )}
        </section>

        <section class="settings-section settings-section--recognition">
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
          ${renderSttModelPicker(
            values.local_stt_family,
            values.local_stt_quant,
            localSttFamilies,
            sttVariantInfo,
            whisperModelDownload,
            downloadProgressPercent,
            sttVariantReady,
          )}

          <label class="field checkbox">
            <input
              name="local_whisper_use_gpu"
              type="checkbox"
              ${values.local_whisper_use_gpu ? "checked" : ""}
              ${localSttGpuCompiled ? "" : "disabled"}
            />
            <span>${escapeHtml(t("settings.whisperUseGpu"))}</span>
            ${
              localSttGpuCompiled
                ? ""
                : `<span class="field-hint">${escapeHtml(t("settings.whisperGpuUnavailable"))}</span>`
            }
          </label>

          ${
            showWhisperBeam
              ? `<label class="field" data-whisper-beam-field>
            <span>${escapeHtml(t("settings.whisperBeamSize"))}</span>
            <select name="local_whisper_beam_size">
              <option value="1" ${values.local_whisper_beam_size === 1 ? "selected" : ""}>${escapeHtml(t("settings.whisperBeamGreedy"))}</option>
              <option value="3" ${values.local_whisper_beam_size === 3 ? "selected" : ""}>${escapeHtml(t("settings.whisperBeam3"))}</option>
              <option value="5" ${values.local_whisper_beam_size === 5 ? "selected" : ""}>${escapeHtml(t("settings.whisperBeam5"))}</option>
            </select>
          </label>`
              : ""
          }
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

        </section>
      </div>

      <div class="tab-panel tab-panel--advanced ${activeTab === "advanced" ? "active" : ""}" data-panel="advanced">
        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.textProcessing"))}</h3>
        <div class="advanced-grid">
          <label class="field advanced-span-2">
            <select
              name="text_rewrite_provider"
              data-text-rewrite-provider
              aria-label="${escapeHtml(t("settings.textRewriteProvider"))}"
            >
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
              <select name="local_llm_model" data-llm-model-select class="model-select-rich llm-model-select">
                ${renderLlmModelOptions(values, llmModels)}
              </select>
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

        ${
          needsOpenAiApiKey(values)
            ? `
        <section class="settings-section" data-openai-connections-section>
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.connections"))}</h3>
          <div class="advanced-grid">
            <label class="field advanced-span-2" data-openai-api-key-panel>
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
        </section>`
            : ""
        }

        <section class="settings-section">
          <h3 class="settings-section-title">${escapeHtml(t("tabs.section.system"))}</h3>
        <div class="advanced-grid">
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
              ${renderSegmentationMsSlider("settings.silenceTimeout", "silence_timeout_ms", values.silence_timeout_ms)}
              ${renderSegmentationMsSlider("settings.vadPreSpeech", "vad_pre_speech_buffer_ms", values.vad_pre_speech_buffer_ms)}
              ${renderSegmentationMsSlider("settings.vadMinimumSpeech", "vad_minimum_speech_ms", values.vad_minimum_speech_ms)}
              ${renderSegmentationMsSlider("settings.vadMaximumSegment", "vad_maximum_segment_ms", values.vad_maximum_segment_ms)}
            </div>
          </details>
        </div>

        <div class="toggle-grid advanced-toggles">
          <label class="field checkbox">
            <input name="spoken_punctuation" type="checkbox" ${values.spoken_punctuation ? "checked" : ""} />
            <span>${escapeHtml(t("settings.spokenPunctuation"))}</span>
          </label>

          <div class="field-stack" data-local-only-toggle>
            <label class="field checkbox">
              <input
                name="silero_te"
                type="checkbox"
                ${values.silero_te && sileroTeReady ? "checked" : ""}
                ${sileroTeCompiled && !sileroTeReady ? "disabled" : ""}
              />
              <span>${escapeHtml(t("settings.sileroTe"))}</span>
            </label>
            ${renderSileroTeModelAction(
              values,
              sileroTeCompiled,
              sileroTeReady,
              sileroTeModelDownload,
              sileroTeDownloadProgressPercent,
            )}
          </div>

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
    recording_indicator: data.get("recording_indicator") === "on",
    abort_on_focus_loss: data.get("abort_on_focus_loss") === "on",
    microphone_device: String(data.get("microphone_device") ?? ""),
    language: String(data.get("language") ?? "auto"),
    injection_mode: String(data.get("injection_mode") ?? "auto") as InjectionMode,
    text_processing_mode: String(
      data.get("text_processing_mode") ?? "basic",
    ) as TextProcessingMode,
    spoken_punctuation: data.get("spoken_punctuation") === "on",
    silero_te: (() => {
      const input = form.querySelector<HTMLInputElement>('input[name="silero_te"]');
      if (input?.disabled) {
        return false;
      }
      return data.get("silero_te") === "on";
    })(),
    numbers_as_words: data.get("numbers_as_words") === "on",
    emulate_enter: data.get("emulate_enter") === "on",
    enter_trigger_phrase: String(data.get("enter_trigger_phrase") ?? ""),
    start_on_boot: data.get("start_on_boot") === "on",
    check_updates_on_startup: data.get("check_updates_on_startup") === "on",
    capslock_ptt: data.get("capslock_ptt") === "on",
    show_notifications: (() => {
      const input = form.querySelector<HTMLInputElement>('input[name="show_notifications"]');
      if (input?.disabled) {
        return false;
      }
      return data.get("show_notifications") === "on";
    })(),
    silence_timeout_ms: clampSegmentationMs(
      "silence_timeout_ms",
      Number(data.get("silence_timeout_ms") ?? 700),
    ),
    ui_locale: String(data.get("ui_locale") ?? "en") as UiLocale,
    transcription_provider: String(data.get("transcription_provider") ?? "local"),
    local_stt_model: String(
      data.get("local_stt_model") ?? data.get("local_whisper_model") ?? "base",
    ) as LocalSttModelKind,
    local_stt_family: String(data.get("local_stt_family") ?? "whisper_base") as LocalSttFamily,
    local_stt_quant: readLocalSttQuantFromForm(form),
    whisper_model_exists: false,
    local_whisper_use_gpu: data.get("local_whisper_use_gpu") === "on",
    local_whisper_beam_size: (() => {
      const draft = {
        transcription_provider: String(data.get("transcription_provider") ?? "local"),
        local_stt_family: String(data.get("local_stt_family") ?? "whisper_base") as LocalSttFamily,
        local_stt_model: String(
          data.get("local_stt_model") ?? data.get("local_whisper_model") ?? "base",
        ) as LocalSttModelKind,
        local_stt_quant: readLocalSttQuantFromForm(form),
      };
      const diagnostics = getState().diagnostics;
      if (
        isLocalWhisperBeamSizeActive(draft, {
          whisperCompiled: diagnostics?.whisper_local_compiled !== false,
        })
      ) {
        return Number(data.get("local_whisper_beam_size") ?? 1);
      }
      return getState().settings?.local_whisper_beam_size ?? 1;
    })(),
    text_rewrite_provider: String(
      data.get("text_rewrite_provider") ?? "openai",
    ) as TextRewriteProvider,
    local_llm_model: String(data.get("local_llm_model") ?? "qwen3_4b") as LlmModelKind,
    llm_model_exists: false,
    local_llm_use_gpu: data.get("local_llm_use_gpu") === "on",
    ai_rewrite_skill: String(data.get("ai_rewrite_skill") ?? ""),
    whisper_prompt_prefix: String(data.get("whisper_prompt_prefix") ?? ""),
    data_storage_dir: getState().dataStorageDir.trim(),
    audio_preprocess_enabled: data.get("audio_preprocess_enabled") === "on",
    audio_noise_reduction_enabled: data.get("audio_noise_reduction_enabled") === "on",
    vad_pre_speech_buffer_ms: clampSegmentationMs(
      "vad_pre_speech_buffer_ms",
      Number(data.get("vad_pre_speech_buffer_ms") ?? 300),
    ),
    vad_minimum_speech_ms: clampSegmentationMs(
      "vad_minimum_speech_ms",
      Number(data.get("vad_minimum_speech_ms") ?? 250),
    ),
    vad_maximum_segment_ms: clampSegmentationMs(
      "vad_maximum_segment_ms",
      Number(data.get("vad_maximum_segment_ms") ?? 30_000),
    ),
    vad_engine: (String(data.get("vad_engine") ?? "silero") === "webrtc"
      ? "webrtc"
      : "silero") as VadEngine,
    vad_threshold_mode: (String(data.get("vad_threshold_mode") ?? "auto") === "manual"
      ? "manual"
      : "auto") as VadThresholdMode,
    vad_voice_threshold_percent: Number(data.get("vad_voice_threshold_percent") ?? 15),
    vad_auto_threshold_percent: Number(
      data.get("vad_auto_threshold_percent") ?? 12,
    ),
    stt_idle_unload_sec: clampIdleUnloadSec(Number(data.get("stt_idle_unload_sec") ?? 180)),
    llm_idle_unload_sec: clampIdleUnloadSec(Number(data.get("llm_idle_unload_sec") ?? 90)),
    prewarm_local_models_at_startup: data.get("prewarm_local_models_at_startup") === "on",
    ...readWeakPcFormFields(form),
    api_key: String(data.get("api_key") ?? ""),
    has_api_key: false,
  };
}

export function textModeHintKey(mode: TextProcessingMode): MessageKey {
  return TEXT_MODE_HINT_KEYS[mode];
}

const DEVICE_LABEL_MAX = 34;

/** Matches validation in `src-tauri/src/settings/config.rs`. */
const SEGMENTATION_MS_LIMITS = {
  silence_timeout_ms: { min: 200, max: 10_000, step: 50 },
  vad_pre_speech_buffer_ms: { min: 50, max: 2_000, step: 50 },
  vad_minimum_speech_ms: { min: 50, max: 2_000, step: 50 },
  vad_maximum_segment_ms: { min: 1_000, max: 120_000, step: 1_000 },
} as const;

type SegmentationMsField = keyof typeof SEGMENTATION_MS_LIMITS;

function clampSegmentationMs(name: SegmentationMsField, value: number): number {
  const { min, max, step } = SEGMENTATION_MS_LIMITS[name];
  const clamped = Math.min(max, Math.max(min, Number.isFinite(value) ? value : min));
  const steps = Math.round((clamped - min) / step);
  return min + steps * step;
}

export function formatSegmentationMsValue(ms: number): string {
  if (ms >= 60_000 && ms % 60_000 === 0) {
    return `${ms / 60_000} min`;
  }
  if (ms >= 1_000) {
    if (ms % 1_000 === 0) {
      return `${ms / 1_000} s`;
    }
    const sec = Math.round((ms / 1_000) * 10) / 10;
    return `${String(sec).replace(/\.0$/, "")} s`;
  }
  return `${ms} ms`;
}

function renderSegmentationMsSlider(
  labelKey: MessageKey,
  name: SegmentationMsField,
  value: number,
): string {
  const { min, max, step } = SEGMENTATION_MS_LIMITS[name];
  const snapped = clampSegmentationMs(name, value);
  const minLabel = formatSegmentationMsValue(min);
  const maxLabel = formatSegmentationMsValue(max);
  return `
    <label class="field segmentation-slider-field">
      <div class="segmentation-slider-header">
        <span>${escapeHtml(t(labelKey))}</span>
        <span class="segmentation-slider-value" data-ms-value-for="${name}">${escapeHtml(formatSegmentationMsValue(snapped))}</span>
      </div>
      <input
        type="range"
        name="${name}"
        class="segmentation-slider"
        data-ms-slider
        min="${min}"
        max="${max}"
        step="${step}"
        value="${snapped}"
      />
      <div class="segmentation-slider-bounds" aria-hidden="true">
        <span>${escapeHtml(minLabel)}</span>
        <span>${escapeHtml(maxLabel)}</span>
      </div>
    </label>
  `;
}

/** Matches `AppSettings::idle_unload_sec_valid` in `src-tauri/src/settings/config.rs`. */
const IDLE_UNLOAD_SEC = {
  never: 0,
  min: 60,
  max: 7_200,
  step: 30,
} as const;

type IdleUnloadField = "stt_idle_unload_sec" | "llm_idle_unload_sec";

function clampIdleUnloadSec(value: number): number {
  if (value === 0) {
    return 0;
  }
  const { min, max, step } = IDLE_UNLOAD_SEC;
  const clamped = Math.min(max, Math.max(min, Number.isFinite(value) ? value : min));
  return Math.round(clamped / step) * step;
}

export function formatIdleUnloadSecValue(sec: number): string {
  const snapped = clampIdleUnloadSec(sec);
  if (snapped === 0) {
    return t("settings.idleUnloadNever");
  }
  if (snapped >= 3_600 && snapped % 3_600 === 0) {
    const hours = snapped / 3_600;
    return hours === 1 ? "1 h" : `${hours} h`;
  }
  if (snapped >= 60 && snapped % 60 === 0) {
    return `${snapped / 60} min`;
  }
  return t("settings.idleUnloadDurationSec", { sec: String(snapped) });
}

function renderIdleUnloadSlider(
  labelKey: MessageKey,
  name: IdleUnloadField,
  value: number,
): string {
  const snapped = clampIdleUnloadSec(value);
  const { never, max, step } = IDLE_UNLOAD_SEC;
  return `
    <label class="field segmentation-slider-field status-quick-settings-span">
      <div class="segmentation-slider-header">
        <span>${escapeHtml(t(labelKey))}</span>
        <span class="segmentation-slider-value" data-ms-value-for="${name}">${escapeHtml(formatIdleUnloadSecValue(snapped))}</span>
      </div>
      <input
        type="range"
        name="${name}"
        class="segmentation-slider"
        data-idle-unload-slider
        min="${never}"
        max="${max}"
        step="${step}"
        value="${snapped}"
      />
      <div class="segmentation-slider-bounds" aria-hidden="true">
        <span>${escapeHtml(t("settings.idleUnloadBoundNever"))}</span>
        <span>${escapeHtml(t("settings.idleUnloadBoundMax"))}</span>
      </div>
    </label>
  `;
}

const WEAK_PC_RAM_CAP = { min: 1, max: 8, step: 1 } as const;
const WEAK_PC_DISK_MB = { min: 50, max: 4_096, step: 50 } as const;

export type WeakPcFormValues = Pick<
  SettingsFormValues,
  | "weak_pc_mode"
  | "weak_pc_spill_to_disk"
  | "weak_pc_ram_segment_cap"
  | "weak_pc_max_disk_queue_mb"
  | "weak_pc_reduce_prewarm"
>;

export function weakPcValuesFromSettings(settings: AppSettings): WeakPcFormValues {
  return {
    weak_pc_mode: settings.weak_pc_mode ?? false,
    weak_pc_spill_to_disk: settings.weak_pc_spill_to_disk ?? true,
    weak_pc_ram_segment_cap: settings.weak_pc_ram_segment_cap ?? 2,
    weak_pc_max_disk_queue_mb: settings.weak_pc_max_disk_queue_mb ?? 512,
    weak_pc_reduce_prewarm: settings.weak_pc_reduce_prewarm ?? true,
  };
}

export const DEFAULT_WEAK_PC_FORM: WeakPcFormValues = {
  weak_pc_mode: false,
  weak_pc_spill_to_disk: true,
  weak_pc_ram_segment_cap: 2,
  weak_pc_max_disk_queue_mb: 512,
  weak_pc_reduce_prewarm: true,
};

export function readWeakPcFormFields(form: HTMLFormElement): WeakPcFormValues {
  const mode = form.querySelector<HTMLInputElement>('input[name="weak_pc_mode"]')?.checked ?? false;
  const hasExpertOptions = Boolean(form.querySelector("[data-weak-pc-options]"));
  if (!hasExpertOptions) {
    const current = getState().settings;
    const base = current ? weakPcValuesFromSettings(current) : { ...DEFAULT_WEAK_PC_FORM };
    if (!mode) {
      return { ...base, weak_pc_mode: false };
    }
    return { ...DEFAULT_WEAK_PC_FORM, weak_pc_mode: true };
  }

  const data = new FormData(form);
  return {
    weak_pc_mode: mode,
    weak_pc_spill_to_disk: data.get("weak_pc_spill_to_disk") === "on",
    weak_pc_ram_segment_cap: clampWeakPcRamCap(Number(data.get("weak_pc_ram_segment_cap") ?? 2)),
    weak_pc_max_disk_queue_mb: clampWeakPcDiskMb(
      Number(data.get("weak_pc_max_disk_queue_mb") ?? 512),
    ),
    weak_pc_reduce_prewarm: data.get("weak_pc_reduce_prewarm") === "on",
  };
}

/** Expert UI: switch + advanced options (no long hint). */
export function renderWeakPcExpertSettings(values: WeakPcFormValues): string {
  return `
          <div class="status-quick-settings-subsection weak-pc-panel weak-pc-panel--in-popover">
            <label class="field checkbox status-quick-settings-span">
              <input name="weak_pc_mode" type="checkbox" ${values.weak_pc_mode ? "checked" : ""} />
              <span>${escapeHtml(t("settings.weakPcMode"))}</span>
            </label>
            <div class="weak-pc-options" data-weak-pc-options ${values.weak_pc_mode ? "" : "hidden"}>
              <label class="field checkbox status-quick-settings-span">
                <input name="weak_pc_spill_to_disk" type="checkbox" ${values.weak_pc_spill_to_disk ? "checked" : ""} />
                <span>${escapeHtml(t("settings.weakPcSpillToDisk"))}</span>
              </label>
              <label class="field checkbox status-quick-settings-span">
                <input name="weak_pc_reduce_prewarm" type="checkbox" ${values.weak_pc_reduce_prewarm ? "checked" : ""} />
                <span>${escapeHtml(t("settings.weakPcReducePrewarm"))}</span>
              </label>
              ${renderWeakPcRamSlider(values.weak_pc_ram_segment_cap, true)}
              ${renderWeakPcDiskMbSlider(values.weak_pc_max_disk_queue_mb, true)}
            </div>
          </div>
  `;
}

/** Standard (homemaker) UI: single switch only. */
export function renderWeakPcHomemakerSwitch(checked: boolean): string {
  return `
            <label class="field checkbox homemaker-weak-pc-toggle">
              <input name="weak_pc_mode" type="checkbox" ${checked ? "checked" : ""} />
              <span>${escapeHtml(t("settings.weakPcMode"))}</span>
            </label>
  `;
}

function clampWeakPcRamCap(value: number): number {
  const v = Number.isFinite(value) ? Math.round(value) : WEAK_PC_RAM_CAP.min;
  return Math.min(WEAK_PC_RAM_CAP.max, Math.max(WEAK_PC_RAM_CAP.min, v));
}

function clampWeakPcDiskMb(value: number): number {
  const v = Number.isFinite(value) ? Math.round(value) : 512;
  const clamped = Math.min(WEAK_PC_DISK_MB.max, Math.max(WEAK_PC_DISK_MB.min, v));
  const steps = Math.round((clamped - WEAK_PC_DISK_MB.min) / WEAK_PC_DISK_MB.step);
  return WEAK_PC_DISK_MB.min + steps * WEAK_PC_DISK_MB.step;
}

function renderWeakPcRamSlider(value: number, inQuickSettings = false): string {
  const snapped = clampWeakPcRamCap(value);
  const spanClass = inQuickSettings ? " status-quick-settings-span" : "";
  return `
    <label class="field segmentation-slider-field${spanClass}">
      <div class="segmentation-slider-header">
        <span>${escapeHtml(t("settings.weakPcRamSegmentCap"))}</span>
        <span class="segmentation-slider-value" data-ms-value-for="weak_pc_ram_segment_cap">${snapped}</span>
      </div>
      <input
        type="range"
        name="weak_pc_ram_segment_cap"
        class="segmentation-slider"
        data-weak-pc-slider
        min="${WEAK_PC_RAM_CAP.min}"
        max="${WEAK_PC_RAM_CAP.max}"
        step="${WEAK_PC_RAM_CAP.step}"
        value="${snapped}"
      />
    </label>
  `;
}

function renderWeakPcDiskMbSlider(value: number, inQuickSettings = false): string {
  const snapped = clampWeakPcDiskMb(value);
  const spanClass = inQuickSettings ? " status-quick-settings-span" : "";
  return `
    <label class="field segmentation-slider-field${spanClass}">
      <div class="segmentation-slider-header">
        <span>${escapeHtml(t("settings.weakPcMaxDiskQueueMb"))}</span>
        <span class="segmentation-slider-value" data-ms-value-for="weak_pc_max_disk_queue_mb">${snapped} MB</span>
      </div>
      <input
        type="range"
        name="weak_pc_max_disk_queue_mb"
        class="segmentation-slider"
        data-weak-pc-slider
        min="${WEAK_PC_DISK_MB.min}"
        max="${WEAK_PC_DISK_MB.max}"
        step="${WEAK_PC_DISK_MB.step}"
        value="${snapped}"
      />
    </label>
  `;
}

export function bindSegmentationMsSliders(form: HTMLFormElement): void {
  form.querySelectorAll<HTMLInputElement>("[data-ms-slider]").forEach((slider) => {
    const update = (): void => {
      const label = form.querySelector<HTMLElement>(`[data-ms-value-for="${slider.name}"]`);
      if (label) {
        label.textContent = formatSegmentationMsValue(Number(slider.value));
      }
    };
    slider.addEventListener("input", update);
  });

  form.querySelectorAll<HTMLInputElement>("[data-idle-unload-slider]").forEach((slider) => {
    const update = (): void => {
      const sec = clampIdleUnloadSec(Number(slider.value));
      if (String(sec) !== slider.value) {
        slider.value = String(sec);
      }
      const label = form.querySelector<HTMLElement>(`[data-ms-value-for="${slider.name}"]`);
      if (label) {
        label.textContent = formatIdleUnloadSecValue(sec);
      }
    };
    slider.addEventListener("input", update);
  });

  form.querySelectorAll<HTMLInputElement>("[data-weak-pc-slider]").forEach((slider) => {
    const update = (): void => {
      const isRam = slider.name === "weak_pc_ram_segment_cap";
      const value = isRam
        ? clampWeakPcRamCap(Number(slider.value))
        : clampWeakPcDiskMb(Number(slider.value));
      if (String(value) !== slider.value) {
        slider.value = String(value);
      }
      const label = form.querySelector<HTMLElement>(`[data-ms-value-for="${slider.name}"]`);
      if (label) {
        label.textContent = isRam ? String(value) : `${value} MB`;
      }
    };
    slider.addEventListener("input", update);
  });
}

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
