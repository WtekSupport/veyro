import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AppState =
  | "initializing"
  | "disabled"
  | "ready"
  | "listening"
  | "processing"
  | "transcribing"
  | "injecting"
  | "error"
  | "permission_required"
  | "microphone_unavailable"
  | "network_unavailable";

export type InjectionMode = "paste" | "keyboard" | "auto";
export type UiLocale = "en" | "ru";
export type UiMode = "expert" | "homemaker";
export type VadThresholdMode = "auto" | "manual";
export type HomemakerDataStorage = "cloud" | "local";
export type TextProcessingMode =
  | "original"
  | "basic"
  | "optimization"
  | "custom_skill";

export type WhisperModelKind =
  | "base"
  | "small"
  | "medium"
  | "large_v3_turbo"
  | "large_v3";
export type TextRewriteProvider = "openai" | "local";
export type LlmModelKind = "qwen3_4b" | "t_lite_it21" | "qwen25_7b" | "gec08b";

export interface StatusSnapshot {
  state: AppState;
  enabled: boolean;
  push_to_talk: boolean;
  ptt_hold: boolean;
  hotkey: string;
  microphone_device: string | null;
  language: string | null;
  injection_mode: InjectionMode;
  last_error: string | null;
  injection_available: boolean;
  injection_backend: string;
}

export interface AppSettings {
  enabled: boolean;
  global_hotkey: string;
  push_to_talk: boolean;
  ptt_hold: boolean;
  live_dictation_field_indicator: boolean;
  hotkey_game_mode: boolean;
  hotkey_block_system: boolean;
  microphone_device: string | null;
  language: string | null;
  transcription_provider: string;
  transcription_model: string;
  local_whisper_models_dir: string | null;
  local_whisper_model: WhisperModelKind;
  local_whisper_use_gpu: boolean;
  local_whisper_beam_size: number;
  text_rewrite_provider: TextRewriteProvider;
  local_llm_model: LlmModelKind;
  local_llm_models_dir: string | null;
  local_llm_use_gpu: boolean;
  ai_rewrite_skill: string | null;
  whisper_prompt_prefix: string;
  transcription_dictionary_path: string | null;
  audio_preprocess_enabled: boolean;
  audio_noise_reduction_enabled: boolean;
  vad_pre_speech_buffer_ms: number;
  vad_minimum_speech_ms: number;
  vad_maximum_segment_ms: number;
  injection_mode: InjectionMode;
  spoken_punctuation: boolean;
  text_processing_mode: TextProcessingMode;
  numbers_as_words: boolean;
  emulate_enter: boolean;
  enter_trigger_phrase: string;
  start_on_boot: boolean;
  check_updates_on_startup: boolean;
  capslock_ptt: boolean;
  show_notifications: boolean;
  log_level: string;
  silence_timeout_ms: number;
  vad_threshold_mode: VadThresholdMode;
  vad_voice_threshold_percent: number;
  vad_auto_threshold_percent: number;
  ui_locale: UiLocale;
  ui_mode: UiMode;
}

export interface SettingsPatch {
  enabled?: boolean;
  global_hotkey?: string;
  push_to_talk?: boolean;
  ptt_hold?: boolean;
  live_dictation_field_indicator?: boolean;
  hotkey_game_mode?: boolean;
  hotkey_block_system?: boolean;
  microphone_device?: string | null;
  language?: string | null;
  transcription_provider?: string;
  transcription_model?: string;
  local_whisper_models_dir?: string | null;
  local_whisper_model?: WhisperModelKind;
  local_whisper_use_gpu?: boolean;
  local_whisper_beam_size?: number;
  text_rewrite_provider?: TextRewriteProvider;
  local_llm_model?: LlmModelKind;
  local_llm_models_dir?: string | null;
  local_llm_use_gpu?: boolean;
  ai_rewrite_skill?: string | null;
  whisper_prompt_prefix?: string;
  transcription_dictionary_path?: string | null;
  audio_preprocess_enabled?: boolean;
  audio_noise_reduction_enabled?: boolean;
  vad_pre_speech_buffer_ms?: number;
  vad_minimum_speech_ms?: number;
  vad_maximum_segment_ms?: number;
  injection_mode?: InjectionMode;
  spoken_punctuation?: boolean;
  text_processing_mode?: TextProcessingMode;
  numbers_as_words?: boolean;
  emulate_enter?: boolean;
  enter_trigger_phrase?: string;
  start_on_boot?: boolean;
  check_updates_on_startup?: boolean;
  capslock_ptt?: boolean;
  show_notifications?: boolean;
  log_level?: string;
  silence_timeout_ms?: number;
  vad_threshold_mode?: VadThresholdMode;
  vad_voice_threshold_percent?: number;
  vad_auto_threshold_percent?: number;
  ui_locale?: UiLocale;
  ui_mode?: UiMode;
  homemaker_data_storage?: HomemakerDataStorage;
  apply_homemaker_local_setup?: boolean;
}

export interface HomemakerLocalSetup {
  local_whisper_model: WhisperModelKind;
  local_llm_model: LlmModelKind;
  local_whisper_use_gpu: boolean;
  local_llm_use_gpu: boolean;
  whisper_download_needed: boolean;
  llm_download_needed: boolean;
  whisper_size_mb: number;
  llm_size_mb: number;
}

export interface DiagnosticsSnapshot {
  state: AppState;
  audio_running: boolean;
  audio_capturing: boolean;
  push_to_talk: boolean;
  hotkey: string;
  injection_available: boolean;
  injection_backend: string;
  has_api_key: boolean;
  microphone_device: string | null;
  pending_segments: number;
  callbacks_enabled: boolean;
  whisper_backend: string;
  whisper_local_compiled: boolean;
  whisper_gpu_compiled: boolean;
  capslock_ptt_supported: boolean;
  hotkey_game_mode_supported: boolean;
  hotkey_backend: string;
  hook_active: boolean;
  text_processing_mode: string;
  transcription_provider: string;
  text_rewrite_provider: string;
  local_llm_model: string;
  local_llm_compiled: boolean;
  local_llm_gpu_compiled: boolean;
  local_llm_ready: boolean;
  whisper_loaded: boolean;
  llm_loaded: boolean;
  settings_webview_alive: boolean;
  about_webview_alive: boolean;
  process_elevated: boolean;
  hotkeys_blocked_by_elevation: boolean;
}

export interface ActivityLogEntry {
  timestamp_ms: number;
  level: "info" | "warn" | "error";
  message_key: string;
  message_args?: Record<string, unknown>;
}

export interface ErrorPayload {
  code: string;
  message: string;
}

export interface TranscriptionCompletedPayload {
  char_count: number;
}

export interface TranscriptionPartialPayload {
  text: string;
  stable: boolean;
}

export const EVENTS = {
  stateChanged: "app://state-changed",
  listeningStarted: "app://listening-started",
  listeningStopped: "app://listening-stopped",
  transcriptionStarted: "app://transcription-started",
  transcriptionPartial: "app://transcription-partial",
  transcriptionPartialClear: "app://transcription-partial-clear",
  transcriptionCompleted: "app://transcription-completed",
  injectionCompleted: "app://injection-completed",
  error: "app://error",
  microphoneChanged: "app://microphone-changed",
  activityLog: "app://activity-log",
  whisperModelDownloadProgress: "app://whisper-model-download-progress",
  llmModelDownloadProgress: "app://llm-model-download-progress",
  skillImported: "app://skill-imported",
  skillsChanged: "app://skills-changed",
  settingsChanged: "app://settings-changed",
} as const;

export interface WhisperModelDownloadProgress {
  downloaded: number;
  total: number | null;
  percent: number | null;
}

export interface WhisperModelInfo {
  kind: WhisperModelKind;
  path: string;
  exists: boolean;
  size_mb: number;
  selected: boolean;
}

export interface AiSkillInfo {
  filename: string;
  name: string;
  description: string;
}

export async function getStatus(): Promise<StatusSnapshot> {
  return invoke<StatusSnapshot>("get_status");
}

export async function recoverEngine(): Promise<StatusSnapshot> {
  return invoke<StatusSnapshot>("recover_engine");
}

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function updateSettings(
  patch: SettingsPatch,
): Promise<AppSettings> {
  return invoke<AppSettings>("update_settings", { patch });
}

export async function getHomemakerLocalSetup(): Promise<HomemakerLocalSetup> {
  return invoke<HomemakerLocalSetup>("get_homemaker_local_setup");
}

export async function getHomemakerHotkeyPresets(): Promise<string[]> {
  return invoke<string[]>("get_homemaker_hotkey_presets");
}

export async function getDevices(): Promise<string[]> {
  return invoke<string[]>("get_devices");
}

export async function prewarmMicrophone(deviceId: string | null): Promise<void> {
  return invoke<void>("prewarm_microphone", { deviceId });
}

export async function prewarmLocalModels(): Promise<void> {
  return invoke<void>("prewarm_local_models");
}

export async function setApiKey(key: string): Promise<void> {
  return invoke<void>("set_api_key", { key });
}

export async function clearApiKey(): Promise<void> {
  return invoke<void>("clear_api_key_command");
}

export async function hasApiKey(): Promise<boolean> {
  return invoke<boolean>("has_api_key_command");
}

export async function getDiagnostics(): Promise<DiagnosticsSnapshot> {
  return invoke<DiagnosticsSnapshot>("get_diagnostics");
}

export async function ensureMicMonitor(): Promise<void> {
  await invoke("ensure_mic_monitor");
}

export async function getMicLevel(): Promise<number> {
  return invoke<number>("get_mic_level");
}

export interface MicMonitorSnapshot {
  level_percent: number;
  speech_active: boolean;
  effective_threshold_percent: number;
  history: number[];
}

export async function getMicMonitorSnapshot(): Promise<MicMonitorSnapshot> {
  return invoke<MicMonitorSnapshot>("get_mic_monitor_snapshot");
}

export async function calibrateVadThreshold(): Promise<number> {
  return invoke<number>("calibrate_vad_threshold");
}

export async function getActivityLog(): Promise<ActivityLogEntry[]> {
  return invoke<ActivityLogEntry[]>("get_activity_log");
}

export async function clearActivityLog(): Promise<void> {
  return invoke<void>("clear_activity_log");
}

export interface WhisperModelStatus {
  path: string;
  exists: boolean;
}

export async function getWhisperModelStatus(): Promise<WhisperModelStatus> {
  return invoke<WhisperModelStatus>("get_whisper_model_status");
}

export interface TranscriptionLanguageInfo {
  code: string;
  name: string;
}

export async function listTranscriptionLanguages(): Promise<TranscriptionLanguageInfo[]> {
  return invoke<TranscriptionLanguageInfo[]>("list_transcription_languages");
}

export async function listWhisperModels(): Promise<WhisperModelInfo[]> {
  return invoke<WhisperModelInfo[]>("list_whisper_models");
}

export async function downloadWhisperModel(model: WhisperModelKind): Promise<string> {
  return invoke<string>("download_whisper_model", { model });
}

export async function getWhisperModelsDir(): Promise<string> {
  return invoke<string>("get_whisper_models_dir");
}

export async function pickWhisperModelsDir(): Promise<string | null> {
  return invoke<string | null>("pick_whisper_models_dir");
}

export interface LlmModelDownloadProgress {
  downloaded: number;
  total: number | null;
  percent: number | null;
}

export interface LlmModelInfo {
  kind: LlmModelKind;
  path: string;
  exists: boolean;
  size_mb: number;
  selected: boolean;
}

export interface LlmModelStatus {
  path: string;
  exists: boolean;
}

export async function getLlmModelStatus(): Promise<LlmModelStatus> {
  return invoke<LlmModelStatus>("get_llm_model_status");
}

export async function listLlmModels(): Promise<LlmModelInfo[]> {
  return invoke<LlmModelInfo[]>("list_llm_models");
}

export async function downloadLlmModel(model: LlmModelKind): Promise<string> {
  return invoke<string>("download_llm_model", { model });
}

export async function getLlmModelsDir(): Promise<string> {
  return invoke<string>("get_llm_models_dir");
}

export async function pickLlmModelsDir(): Promise<string | null> {
  return invoke<string | null>("pick_llm_models_dir");
}

export async function listAiSkills(): Promise<AiSkillInfo[]> {
  return invoke<AiSkillInfo[]>("list_ai_skills");
}

export async function openAiSkillsFolder(): Promise<void> {
  return invoke<void>("open_ai_skills_folder");
}

export async function importAiSkill(fromPath: string): Promise<AiSkillInfo> {
  return invoke<AiSkillInfo>("import_ai_skill", { fromPath });
}

export async function pickAndImportAiSkill(): Promise<AiSkillInfo> {
  return invoke<AiSkillInfo>("pick_and_import_ai_skill");
}

export interface AppInfo {
  version_display: string;
  version_semver: string;
  hwid_hash: string;
  publisher: string;
  copyright: string;
}

export interface ThirdPartyLicense {
  name: string;
  copyright: string;
  license: string;
  url: string;
}

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export async function getThirdPartyLicenses(): Promise<ThirdPartyLicense[]> {
  return invoke<ThirdPartyLicense[]>("get_third_party_licenses");
}

export async function openAboutWindow(): Promise<void> {
  return invoke<void>("open_about_window");
}

export async function getDictionaryPath(): Promise<string> {
  return invoke<string>("get_dictionary_path");
}

export async function openTranscriptionDictionaryFolder(): Promise<void> {
  return invoke<void>("open_transcription_dictionary_folder");
}

export async function pickTranscriptionDictionary(): Promise<string | null> {
  return invoke<string | null>("pick_transcription_dictionary");
}

export async function subscribe<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(event, (event) => handler(event.payload));
}
