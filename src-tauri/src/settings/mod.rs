pub mod config;
pub mod data_storage;
pub mod encryption;
pub mod homemaker;
pub mod local_stt;
pub mod secrets;
pub mod stt_catalog;
pub mod storage;

pub use config::{
    local_llm_compiled, local_llm_gpu_backend_label, local_llm_gpu_compiled,
    whisper_gpu_backend_label, whisper_gpu_compiled, whisper_local_compiled, AppSettings,
    HomemakerDataStorage, InjectionMode, LlmModelKind, SettingsPatch, TextProcessingMode,
    TextRewriteProvider, UiLocale, UiMode, VadEngine, VadThresholdMode, WhisperModelKind,
    effective_vad_engine, vad_silero_compiled, vad_silero_runtime_available,
};
pub use local_stt::{
    local_stt_gpu_compiled, sherpa_accelerator_available, sherpa_gpu_compiled, sherpa_stt_compiled,
    LocalSttEngine, LocalSttModelKind,
};
pub use stt_catalog::{
    migrate_from_legacy_stt_model, normalize_variant, parse_variant_id, quants_for_family,
    variant_spec, whisper_download_url_for, LocalSttFamily, LocalSttQuant, LocalSttVariant,
    SherpaOnnxLayout, SttVariantSpec,
};
pub use homemaker::{apply_homemaker_patch_side_effects, normalize_homemaker_settings};
pub use secrets::{clear_api_key, has_api_key, load_api_key, save_api_key};
pub use data_storage::{
    default_data_storage_root, ensure_data_storage_layout, normalize_data_storage,
    resolve_data_storage_root, resolve_dictionary_path as resolve_dictionary_file_path,
    resolve_llm_models_dir, resolve_stt_models_dir, DICTIONARY_FILE, SUBDIR_DICTIONARY,
    SUBDIR_LLM_MODELS, SUBDIR_STT_MODELS,
};
pub use storage::{load_settings, normalize_locale_dependent_settings, save_settings};
