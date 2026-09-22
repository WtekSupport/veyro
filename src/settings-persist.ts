import {
  getDiagnostics,
  getStatus,
  getLlmModelStatus,
  getDataStorageDir,
  getLlmModelsDir,
  getWhisperModelStatus,
  getDictionaryPath,
  getWhisperModelsDir,
  getSileroTeModelStatus,
  getSileroVadModelStatus,
  listLlmModels,
  describeLocalSttVariant,
  effectiveLocalSttFamily,
  effectiveLocalSttQuant,
  isLocalWhisperBeamSizeActive,
  listLocalSttModels,
  setApiKey,
  updateSettings,
  type AppSettings,
  type SettingsPatch,
} from "./api";
import { notifyMicDeviceChanged } from "./components/mic-meter";
import { notifyVadMicDeviceChanged } from "./components/vad-threshold-panel";
import { readSettingsForm, type SettingsFormValues } from "./components/settings";
import { setLocale } from "./i18n";
import { getState, patchState, setError, setSettings, setStatus } from "./state";
import { translateError } from "./i18n";

export function formValuesToPatch(values: SettingsFormValues): SettingsPatch {
  const hasApiKey = getState().hasApiKey;
  const transcriptionProvider =
    !hasApiKey && values.transcription_provider === "openai"
      ? "local"
      : values.transcription_provider;

  const current = getState().settings;
  const diagnostics = getState().diagnostics;
  const beamActive = isLocalWhisperBeamSizeActive(values, {
    whisperCompiled: diagnostics?.whisper_local_compiled !== false,
  });

  return {
    global_hotkey: values.global_hotkey,
    push_to_talk: values.push_to_talk,
    ptt_hold: values.push_to_talk
      ? values.ptt_hold
      : (current?.ptt_hold ?? true),
    recording_indicator: values.recording_indicator,
    abort_on_focus_loss: values.abort_on_focus_loss,
    microphone_device:
      values.microphone_device.length > 0 ? values.microphone_device : null,
    language: values.language === "auto" ? null : values.language,
    transcription_provider: transcriptionProvider,
    local_stt_family: values.local_stt_family,
    local_stt_quant: values.local_stt_quant,
    local_whisper_use_gpu: values.local_whisper_use_gpu,
    local_whisper_beam_size: beamActive
      ? values.local_whisper_beam_size
      : (current?.local_whisper_beam_size ?? values.local_whisper_beam_size),
    text_rewrite_provider: values.text_rewrite_provider,
    local_llm_model: values.local_llm_model,
    local_llm_use_gpu: values.local_llm_use_gpu,
    ai_rewrite_skill:
      values.ai_rewrite_skill.length > 0 ? values.ai_rewrite_skill : null,
    whisper_prompt_prefix: values.whisper_prompt_prefix,
    audio_preprocess_enabled: values.audio_preprocess_enabled,
    audio_noise_reduction_enabled: values.audio_noise_reduction_enabled,
    vad_pre_speech_buffer_ms: values.vad_pre_speech_buffer_ms,
    vad_minimum_speech_ms: values.vad_minimum_speech_ms,
    vad_maximum_segment_ms: values.vad_maximum_segment_ms,
    injection_mode: values.injection_mode,
    text_processing_mode: values.text_processing_mode,
    spoken_punctuation: values.spoken_punctuation,
    silero_te: values.silero_te,
    numbers_as_words: values.numbers_as_words,
    emulate_enter: values.emulate_enter,
    enter_trigger_phrase: values.enter_trigger_phrase,
    start_on_boot: values.start_on_boot,
    check_updates_on_startup: values.check_updates_on_startup,
    capslock_ptt: values.capslock_ptt,
    show_notifications: values.show_notifications,
    silence_timeout_ms: values.silence_timeout_ms,
    vad_engine: values.vad_engine,
    vad_threshold_mode: values.vad_threshold_mode,
    vad_voice_threshold_percent: values.vad_voice_threshold_percent,
    vad_auto_threshold_percent: values.vad_auto_threshold_percent,
    ui_locale: values.ui_locale,
    stt_idle_unload_sec: values.stt_idle_unload_sec,
    llm_idle_unload_sec: values.llm_idle_unload_sec,
    prewarm_local_models_at_startup: values.prewarm_local_models_at_startup,
    weak_pc_mode: values.weak_pc_mode,
    weak_pc_spill_to_disk: values.weak_pc_spill_to_disk,
    weak_pc_ram_segment_cap: values.weak_pc_ram_segment_cap,
    weak_pc_max_disk_queue_mb: values.weak_pc_max_disk_queue_mb,
    weak_pc_reduce_prewarm: values.weak_pc_reduce_prewarm,
  };
}

function settingsChanged(values: SettingsFormValues, current: AppSettings): boolean {
  const patch = formValuesToPatch(values);
  return (
    patch.global_hotkey !== current.global_hotkey ||
    patch.push_to_talk !== current.push_to_talk ||
    patch.ptt_hold !== current.ptt_hold ||
    patch.recording_indicator !==
      (current.recording_indicator ?? current.live_dictation_field_indicator) ||
    patch.abort_on_focus_loss !== (current.abort_on_focus_loss ?? false) ||
    patch.microphone_device !== current.microphone_device ||
    patch.language !== current.language ||
    patch.transcription_provider !== current.transcription_provider ||
    patch.local_stt_family !== current.local_stt_family ||
    patch.local_stt_quant !== current.local_stt_quant ||
    patch.local_whisper_use_gpu !== current.local_whisper_use_gpu ||
    patch.local_whisper_beam_size !== current.local_whisper_beam_size ||
    patch.text_rewrite_provider !== current.text_rewrite_provider ||
    patch.local_llm_model !== current.local_llm_model ||
    patch.local_llm_use_gpu !== current.local_llm_use_gpu ||
    patch.ai_rewrite_skill !== current.ai_rewrite_skill ||
    patch.whisper_prompt_prefix !== current.whisper_prompt_prefix ||
    patch.audio_preprocess_enabled !== current.audio_preprocess_enabled ||
    patch.audio_noise_reduction_enabled !== current.audio_noise_reduction_enabled ||
    patch.vad_pre_speech_buffer_ms !== current.vad_pre_speech_buffer_ms ||
    patch.vad_minimum_speech_ms !== current.vad_minimum_speech_ms ||
    patch.vad_maximum_segment_ms !== current.vad_maximum_segment_ms ||
    patch.injection_mode !== current.injection_mode ||
    patch.text_processing_mode !== current.text_processing_mode ||
    patch.spoken_punctuation !== current.spoken_punctuation ||
    patch.silero_te !== current.silero_te ||
    patch.numbers_as_words !== current.numbers_as_words ||
    patch.emulate_enter !== current.emulate_enter ||
    patch.enter_trigger_phrase !== current.enter_trigger_phrase ||
    patch.start_on_boot !== current.start_on_boot ||
    patch.check_updates_on_startup !== current.check_updates_on_startup ||
    patch.capslock_ptt !== current.capslock_ptt ||
    patch.show_notifications !== current.show_notifications ||
    patch.silence_timeout_ms !== current.silence_timeout_ms ||
    patch.vad_engine !== current.vad_engine ||
    patch.vad_threshold_mode !== current.vad_threshold_mode ||
    patch.vad_voice_threshold_percent !== current.vad_voice_threshold_percent ||
    patch.vad_auto_threshold_percent !== current.vad_auto_threshold_percent ||
    patch.ui_locale !== current.ui_locale ||
    patch.stt_idle_unload_sec !== current.stt_idle_unload_sec ||
    patch.llm_idle_unload_sec !== current.llm_idle_unload_sec ||
    patch.prewarm_local_models_at_startup !== current.prewarm_local_models_at_startup ||
    patch.weak_pc_mode !== current.weak_pc_mode ||
    patch.weak_pc_spill_to_disk !== current.weak_pc_spill_to_disk ||
    patch.weak_pc_ram_segment_cap !== current.weak_pc_ram_segment_cap ||
    patch.weak_pc_max_disk_queue_mb !== current.weak_pc_max_disk_queue_mb ||
    patch.weak_pc_reduce_prewarm !== current.weak_pc_reduce_prewarm ||
    values.api_key.trim().length > 0
  );
}

let persistTimer: ReturnType<typeof setTimeout> | undefined;
let persistInFlight = false;
let persistPending = false;

export function schedulePersistSettings(): void {
  if (persistTimer !== undefined) {
    clearTimeout(persistTimer);
  }
  persistTimer = setTimeout(() => {
    persistTimer = undefined;
    void flushPersistSettings();
  }, 350);
}

export async function flushPersistSettings(options?: {
  compareWith?: AppSettings;
}): Promise<void> {
  const form = document.querySelector<HTMLFormElement>("#settings-form");
  if (!form) {
    return;
  }

  if (persistInFlight) {
    persistPending = true;
    return;
  }

  const current = getState().settings;
  if (!current) {
    return;
  }

  const compareWith = options?.compareWith ?? current;
  const values = readSettingsForm(form);
  if (!settingsChanged(values, compareWith)) {
    return;
  }

  setLocale(values.ui_locale);

  persistInFlight = true;
  try {
    if (values.api_key.trim().length > 0) {
      await setApiKey(values.api_key.trim());
      patchState({ hasApiKey: true });
    }

    const patch = formValuesToPatch(values);
    const micChanged =
      patch.microphone_device !== compareWith.microphone_device;
    const nextSettings = await updateSettings(patch);
    setSettings(nextSettings);
    if (micChanged) {
      notifyMicDeviceChanged();
      notifyVadMicDeviceChanged();
    }
    setLocale(nextSettings.ui_locale);
    setStatus(await getStatus());
    patchState({
      diagnostics: await getDiagnostics(),
      dataStorageDir: await getDataStorageDir(),
      whisperModelsDir: await getWhisperModelsDir(),
      dictionaryPath: await getDictionaryPath(),
      whisperModels: await listLocalSttModels(),
      whisperModel: await getWhisperModelStatus(),
      llmModelsDir: await getLlmModelsDir(),
      llmModels: await listLlmModels(),
      llmModel: await getLlmModelStatus(),
      sileroTeModel: await getSileroTeModelStatus().catch(() => null),
      sileroVadModel: await getSileroVadModelStatus().catch(() => null),
      lastError: null,
    });
    try {
      const sttFamily = effectiveLocalSttFamily(nextSettings);
      const sttQuant = effectiveLocalSttQuant(sttFamily, nextSettings.local_stt_quant);
      const sttVariantInfo = await describeLocalSttVariant(sttFamily, sttQuant);
      patchState({ sttVariantInfo });
    } catch {
      patchState({ sttVariantInfo: null });
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const knownCodes = new Set([
      "hotkey_empty",
      "provider_empty",
      "silence_timeout_range",
      "enter_trigger_phrase_too_long",
      "beam_size_range",
      "vad_pre_speech_range",
      "vad_minimum_speech_range",
      "vad_maximum_segment_range",
    ]);
    setError({
      code: knownCodes.has(message) ? message : "settings",
      message: translateError(message, message),
    });
  } finally {
    persistInFlight = false;
    if (persistPending) {
      persistPending = false;
      void flushPersistSettings();
    }
  }
}
