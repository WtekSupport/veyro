pub mod config;
pub mod encryption;
pub mod homemaker;
pub mod local_stt;
pub mod secrets;
pub mod storage;

pub use config::{
    local_llm_compiled, local_llm_gpu_backend_label, local_llm_gpu_compiled,
    whisper_gpu_backend_label, whisper_gpu_compiled, whisper_local_compiled, AppSettings,
    HomemakerDataStorage, InjectionMode, LlmModelKind, SettingsPatch, TextProcessingMode,
    TextRewriteProvider, UiLocale, UiMode, VadEngine, VadThresholdMode, WhisperModelKind,
    effective_vad_engine, vad_silero_compiled, vad_silero_runtime_available,
};
pub use local_stt::{
    sherpa_accelerator_available, sherpa_gpu_compiled, sherpa_stt_compiled, LocalSttEngine,
    LocalSttModelKind,
};
pub use homemaker::{apply_homemaker_patch_side_effects, normalize_homemaker_settings};
pub use secrets::{clear_api_key, has_api_key, load_api_key, save_api_key};
pub use storage::{load_settings, normalize_locale_dependent_settings, save_settings};
