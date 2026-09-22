use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WhisperModelKind {
    #[default]
    Base,
    Small,
    Medium,
    LargeV3Turbo,
    LargeV3,
}

pub fn whisper_local_compiled() -> bool {
    cfg!(feature = "local-whisper")
}

pub fn whisper_gpu_compiled() -> bool {
    cfg!(any(
        feature = "local-whisper-vulkan",
        feature = "local-whisper-cuda",
        feature = "local-whisper-metal"
    ))
}

#[cfg(feature = "local-whisper-cuda")]
pub fn whisper_gpu_backend_label() -> &'static str {
    "cuda"
}

#[cfg(all(feature = "local-whisper-vulkan", not(feature = "local-whisper-cuda")))]
pub fn whisper_gpu_backend_label() -> &'static str {
    "vulkan"
}

#[cfg(all(
    feature = "local-whisper-metal",
    not(any(feature = "local-whisper-cuda", feature = "local-whisper-vulkan"))
))]
pub fn whisper_gpu_backend_label() -> &'static str {
    "metal"
}

#[cfg(not(any(
    feature = "local-whisper-cuda",
    feature = "local-whisper-vulkan",
    feature = "local-whisper-metal"
)))]
pub fn whisper_gpu_backend_label() -> &'static str {
    "cpu"
}

pub fn local_llm_compiled() -> bool {
    cfg!(feature = "local-llm")
}

pub fn local_llm_gpu_compiled() -> bool {
    cfg!(any(feature = "local-llm-vulkan", feature = "local-llm-cuda"))
}

#[cfg(feature = "local-llm-cuda")]
pub fn local_llm_gpu_backend_label() -> &'static str {
    "cuda"
}

#[cfg(all(feature = "local-llm-vulkan", not(feature = "local-llm-cuda")))]
pub fn local_llm_gpu_backend_label() -> &'static str {
    "vulkan"
}

#[cfg(not(any(feature = "local-llm-vulkan", feature = "local-llm-cuda")))]
pub fn local_llm_gpu_backend_label() -> &'static str {
    "cpu"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VadEngine {
    #[default]
    Silero,
    WebRtc,
}

impl VadEngine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Silero => "silero",
            Self::WebRtc => "webrtc",
        }
    }
}

pub fn effective_vad_engine(engine: VadEngine) -> VadEngine {
    crate::vad::analyzer::effective_engine(engine)
}

pub fn vad_silero_compiled() -> bool {
    crate::vad::analyzer::silero_compiled()
}

pub fn vad_silero_runtime_available() -> bool {
    crate::vad::analyzer::silero_runtime_available()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VadThresholdMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    Expert,
    #[default]
    Homemaker,
}

impl UiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expert => "expert",
            Self::Homemaker => "homemaker",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomemakerDataStorage {
    Cloud,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextRewriteProvider {
    #[default]
    Openai,
    Local,
}

impl TextRewriteProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Local => "local",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LlmModelKind {
    #[default]
    #[serde(rename = "qwen3_4b")]
    Qwen3_4B,
    #[serde(rename = "t_lite_it21")]
    TLiteIt21,
    #[serde(rename = "qwen25_7b")]
    Qwen25_7B,
    #[serde(rename = "gec08b")]
    Gec08B,
}

impl LlmModelKind {
    pub fn all() -> [Self; 4] {
        [Self::Qwen3_4B, Self::TLiteIt21, Self::Qwen25_7B, Self::Gec08B]
    }

    /// Russian-tuned ~5 GB model is offered only when the UI locale is Russian.
    pub fn visible_for_ui_locale(self, locale: UiLocale) -> bool {
        !matches!(self, Self::TLiteIt21) || locale == UiLocale::Ru
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Qwen3_4B => "Qwen_Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
            Self::TLiteIt21 => "T-lite-it-2.1-Q4_K_M.gguf",
            Self::Qwen25_7B => "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
            Self::Gec08B => "Qwen3.5-0.8B-GEC-KAZ-RUS-ENG.Q4_0.gguf",
        }
    }

    pub fn hf_repo(self) -> &'static str {
        match self {
            Self::Qwen3_4B => "bartowski/Qwen_Qwen3-4B-Instruct-2507-GGUF",
            Self::TLiteIt21 => "t-tech/T-lite-it-2.1-GGUF",
            Self::Qwen25_7B => "bartowski/Qwen2.5-7B-Instruct-GGUF",
            Self::Gec08B => "loqira/Qwen3.5-0.8B-GEC-KAZ-RUS-ENG",
        }
    }

    pub fn download_url(self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.hf_repo(),
            self.file_name()
        )
    }

    pub fn approx_size_mb(self) -> u32 {
        match self {
            Self::Qwen3_4B => 2_500,
            Self::TLiteIt21 => 5_000,
            Self::Qwen25_7B => 4_700,
            Self::Gec08B => 500,
        }
    }

    pub fn is_gec(self) -> bool {
        matches!(self, Self::Gec08B)
    }

    /// Qwen3 hybrid models may emit chain-of-thought unless thinking is disabled.
    pub fn supports_hybrid_thinking(self) -> bool {
        matches!(self, Self::Qwen3_4B)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Qwen3_4B => "qwen3_4b",
            Self::TLiteIt21 => "t_lite_it21",
            Self::Qwen25_7B => "qwen25_7b",
            Self::Gec08B => "gec08b",
        }
    }
}

impl WhisperModelKind {
    pub fn all() -> [Self; 5] {
        [
            Self::Base,
            Self::Small,
            Self::Medium,
            Self::LargeV3Turbo,
            Self::LargeV3,
        ]
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::LargeV3Turbo => "ggml-large-v3-turbo.bin",
            Self::LargeV3 => "ggml-large-v3.bin",
        }
    }

    pub fn download_url(self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.file_name()
        )
    }

    pub fn approx_size_mb(self) -> u32 {
        match self {
            Self::Base => 141,
            Self::Small => 466,
            Self::Medium => 1_500,
            Self::LargeV3Turbo => 1_500,
            Self::LargeV3 => 3_100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InjectionMode {
    Paste,
    Keyboard,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UiLocale {
    #[default]
    En,
    Ru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextProcessingMode {
    Original,
    #[default]
    Basic,
    Optimization,
    CustomSkill,
}

impl TextProcessingMode {
    pub fn uses_ai(self) -> bool {
        matches!(self, Self::Optimization | Self::CustomSkill)
    }

    pub fn requires_custom_skill(self) -> bool {
        matches!(self, Self::CustomSkill)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Basic => "basic",
            Self::Optimization => "optimization",
            Self::CustomSkill => "custom_skill",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub enabled: bool,
    pub global_hotkey: String,
    /// macOS: Caps Lock is remapped to F18 and used as the push-to-talk hotkey.
    pub capslock_ptt: bool,
    /// Hotkey to restore when `capslock_ptt` is turned off.
    pub hotkey_before_capslock: Option<String>,
    pub push_to_talk: bool,
    /// When true (default), release the hotkey stops recording. When false, a second press stops.
    #[serde(default = "default_ptt_hold")]
    pub ptt_hold: bool,
    /// Top-right REC overlay while microphone capture is active (PTT or continuous).
    #[serde(
        default = "default_recording_indicator",
        alias = "live_dictation_field_indicator"
    )]
    pub recording_indicator: bool,
    /// Stop queued STT/injection when the dictation field loses focus (Windows only).
    #[serde(default = "default_abort_on_focus_loss")]
    pub abort_on_focus_loss: bool,
    /// Use a low-level keyboard hook so PTT works in exclusive fullscreen games (Windows only).
    #[serde(default = "default_hotkey_game_mode")]
    pub hotkey_game_mode: bool,
    /// Legacy; prefer [`Self::effective_hotkey_block_system`].
    #[serde(default = "default_hotkey_block_system")]
    pub hotkey_block_system: bool,
    pub microphone_device: Option<String>,
    pub language: Option<String>,
    pub transcription_provider: String,
    pub transcription_model: String,
    pub local_whisper_models_dir: Option<String>,
    #[serde(default)]
    pub local_stt_family: crate::settings::LocalSttFamily,
    #[serde(default)]
    pub local_stt_quant: crate::settings::LocalSttQuant,
    #[serde(default, alias = "local_whisper_model")]
    pub local_stt_model: crate::settings::LocalSttModelKind,
    pub local_whisper_use_gpu: bool,
    #[serde(default = "default_local_sherpa_num_threads")]
    pub local_sherpa_num_threads: u32,
    #[serde(default)]
    pub local_whisper_gpu_autodetected: bool,
    pub local_whisper_beam_size: u8,
    pub text_rewrite_provider: TextRewriteProvider,
    pub local_llm_model: LlmModelKind,
    pub local_llm_models_dir: Option<String>,
    pub local_llm_use_gpu: bool,
    #[serde(default)]
    pub local_llm_gpu_autodetected: bool,
    pub ai_rewrite_skill: Option<String>,
    pub whisper_prompt_prefix: String,
    /// Root folder for models, LLM, and dictionary (subfolders applied automatically).
    pub data_storage_dir: Option<String>,
    pub transcription_dictionary_path: Option<String>,
    pub audio_preprocess_enabled: bool,
    pub audio_noise_reduction_enabled: bool,
    pub vad_pre_speech_buffer_ms: u32,
    pub vad_minimum_speech_ms: u32,
    pub vad_maximum_segment_ms: u32,
    pub injection_mode: InjectionMode,
    pub spoken_punctuation: bool,
    /// Silero text enhancement (local STT, Basic/Original only).
    #[serde(
        default = "default_silero_te",
        alias = "auto_punctuation_from_pauses"
    )]
    pub silero_te: bool,
    pub text_processing_mode: TextProcessingMode,
    pub numbers_as_words: bool,
    pub emulate_enter: bool,
    pub enter_trigger_phrase: String,
    pub start_on_boot: bool,
    #[serde(default = "default_check_updates_on_startup")]
    pub check_updates_on_startup: bool,
    pub show_notifications: bool,
    pub log_level: String,
    pub silence_timeout_ms: u32,
    #[serde(default)]
    pub vad_engine: VadEngine,
    #[serde(default)]
    pub vad_threshold_mode: VadThresholdMode,
    #[serde(default = "default_vad_voice_threshold_percent")]
    pub vad_voice_threshold_percent: u8,
    #[serde(default = "default_vad_auto_threshold_percent")]
    pub vad_auto_threshold_percent: u8,
    pub ui_locale: UiLocale,
    #[serde(default)]
    pub ui_mode: UiMode,
    /// Unload local STT after this many seconds without dictation. `0` = never unload.
    #[serde(default = "default_stt_idle_unload_sec")]
    pub stt_idle_unload_sec: u32,
    /// Unload local LLM after this many seconds without dictation. `0` = never unload.
    #[serde(default = "default_llm_idle_unload_sec")]
    pub llm_idle_unload_sec: u32,
    /// Load and warm local STT/LLM at startup (and when the settings UI opens if enabled).
    #[serde(default)]
    pub prewarm_local_models_at_startup: bool,
    /// Low-resource profile: disk spill queue; fewer prewarms while backlogged.
    #[serde(default)]
    pub weak_pc_mode: bool,
    #[serde(default = "default_weak_pc_spill_to_disk")]
    pub weak_pc_spill_to_disk: bool,
    #[serde(default = "default_weak_pc_ram_segment_cap")]
    pub weak_pc_ram_segment_cap: u32,
    #[serde(default = "default_weak_pc_max_disk_queue_mb")]
    pub weak_pc_max_disk_queue_mb: u32,
    #[serde(default = "default_weak_pc_reduce_prewarm")]
    pub weak_pc_reduce_prewarm: bool,
}

fn default_weak_pc_spill_to_disk() -> bool {
    true
}

fn default_weak_pc_ram_segment_cap() -> u32 {
    2
}

fn default_weak_pc_max_disk_queue_mb() -> u32 {
    512
}

fn default_weak_pc_reduce_prewarm() -> bool {
    true
}

fn default_vad_voice_threshold_percent() -> u8 {
    15
}

fn default_vad_auto_threshold_percent() -> u8 {
    12
}

fn default_ptt_hold() -> bool {
    true
}

fn default_check_updates_on_startup() -> bool {
    true
}

fn default_recording_indicator() -> bool {
    true
}

fn default_abort_on_focus_loss() -> bool {
    true
}

fn default_silero_te() -> bool {
    true
}

fn default_hotkey_game_mode() -> bool {
    false
}

fn default_hotkey_block_system() -> bool {
    cfg!(windows)
}

fn default_stt_idle_unload_sec() -> u32 {
    180
}

fn default_llm_idle_unload_sec() -> u32 {
    90
}

fn default_local_sherpa_num_threads() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(4)
        .clamp(1, 8)
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            global_hotkey: "Shift+F1".to_string(),
            capslock_ptt: false,
            hotkey_before_capslock: None,
            push_to_talk: true,
            ptt_hold: true,
            recording_indicator: true,
            abort_on_focus_loss: default_abort_on_focus_loss(),
            hotkey_game_mode: default_hotkey_game_mode(),
            hotkey_block_system: default_hotkey_block_system(),
            microphone_device: None,
            language: None,
            transcription_provider: "local".to_string(),
            transcription_model: "whisper-1".to_string(),
            local_whisper_models_dir: None,
            local_stt_family: crate::settings::LocalSttFamily::WhisperBase,
            local_stt_quant: crate::settings::LocalSttQuant::Legacy,
            local_stt_model: crate::settings::LocalSttModelKind::WhisperBase,
            local_whisper_use_gpu: whisper_gpu_compiled(),
            local_sherpa_num_threads: default_local_sherpa_num_threads(),
            local_whisper_gpu_autodetected: false,
            local_whisper_beam_size: 1,
            text_rewrite_provider: TextRewriteProvider::Openai,
            local_llm_model: LlmModelKind::Qwen3_4B,
            local_llm_models_dir: None,
            local_llm_use_gpu: local_llm_gpu_compiled(),
            local_llm_gpu_autodetected: false,
            ai_rewrite_skill: None,
            whisper_prompt_prefix: String::new(),
            data_storage_dir: None,
            transcription_dictionary_path: None,
            audio_preprocess_enabled: true,
            audio_noise_reduction_enabled: false,
            vad_pre_speech_buffer_ms: 300,
            vad_minimum_speech_ms: 250,
            vad_maximum_segment_ms: 30_000,
            injection_mode: InjectionMode::Auto,
            spoken_punctuation: true,
            silero_te: true,
            text_processing_mode: TextProcessingMode::Original,
            numbers_as_words: false,
            emulate_enter: false,
            enter_trigger_phrase: String::new(),
            start_on_boot: false,
            check_updates_on_startup: true,
            show_notifications: false,
            log_level: "info".to_string(),
            silence_timeout_ms: 700,
            vad_engine: VadEngine::Silero,
            vad_threshold_mode: VadThresholdMode::Auto,
            vad_voice_threshold_percent: default_vad_voice_threshold_percent(),
            vad_auto_threshold_percent: default_vad_auto_threshold_percent(),
            ui_locale: UiLocale::En,
            ui_mode: UiMode::Homemaker,
            stt_idle_unload_sec: default_stt_idle_unload_sec(),
            llm_idle_unload_sec: default_llm_idle_unload_sec(),
            prewarm_local_models_at_startup: false,
            weak_pc_mode: false,
            weak_pc_spill_to_disk: default_weak_pc_spill_to_disk(),
            weak_pc_ram_segment_cap: default_weak_pc_ram_segment_cap(),
            weak_pc_max_disk_queue_mb: default_weak_pc_max_disk_queue_mb(),
            weak_pc_reduce_prewarm: default_weak_pc_reduce_prewarm(),
        }
    }
}

impl AppSettings {
    pub fn local_stt_variant(&self) -> crate::settings::LocalSttVariant {
        crate::settings::normalize_variant(crate::settings::LocalSttVariant::new(
            self.local_stt_family,
            self.local_stt_quant,
        ))
    }

    pub fn set_local_stt_variant(&mut self, variant: crate::settings::LocalSttVariant) {
        let variant = crate::settings::normalize_variant(variant);
        self.local_stt_family = variant.family;
        self.local_stt_quant = variant.quant;
        self.sync_local_stt_model_from_variant();
    }

    pub fn sync_local_stt_model_from_variant(&mut self) {
        if let Some(kind) = self
            .local_stt_family
            .to_legacy_kind(self.local_stt_quant)
        {
            self.local_stt_model = kind;
        }
    }

    /// Align family/quant with `local_stt_model` before path checks (UI may lag controller).
    pub fn repair_local_stt_selection(&mut self) {
        if !self.local_stt_model.is_whisper() && self.local_stt_family.is_whisper() {
            let variant =
                crate::settings::migrate_from_legacy_stt_model(self.local_stt_model);
            self.set_local_stt_variant(variant);
            return;
        }
        self.set_local_stt_variant(crate::settings::normalize_variant(self.local_stt_variant()));
    }

    pub fn stt_idle_unload_after(&self) -> Option<std::time::Duration> {
        idle_unload_duration(self.stt_idle_unload_sec)
    }

    pub fn llm_idle_unload_after(&self) -> Option<std::time::Duration> {
        idle_unload_duration(self.llm_idle_unload_sec)
    }

    pub fn injection_mode_for_host(&self) -> InjectionMode {
        self.injection_mode
    }

    /// PTT "listening" toasts steal focus from fullscreen apps (especially on first press).
    pub fn suppress_ptt_toasts(&self) -> bool {
        self.push_to_talk
    }

    /// Swallow matched hotkey events so they do not reach the focused app (PTT on Windows).
    pub fn effective_hotkey_block_system(&self) -> bool {
        if self.hotkey_block_system {
            return true;
        }
        #[cfg(windows)]
        {
            return self.push_to_talk;
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// Switch the hotkey to the remapped Caps Lock when `capslock_ptt` turns on, and restore the
    /// previous one when it turns off (unless another hotkey was picked in the meantime).
    pub fn sync_capslock_hotkey(&mut self, was_enabled: bool) {
        use crate::hotkey::capslock::HOTKEY;

        if self.capslock_ptt == was_enabled {
            return;
        }
        if self.capslock_ptt {
            if self.global_hotkey != HOTKEY {
                let previous = std::mem::replace(&mut self.global_hotkey, HOTKEY.to_string());
                self.hotkey_before_capslock = Some(previous);
            }
        } else {
            let previous = self.hotkey_before_capslock.take();
            if self.global_hotkey == HOTKEY {
                self.global_hotkey =
                    previous.unwrap_or_else(|| AppSettings::default().global_hotkey);
            }
        }
    }

    pub fn validate(&self) -> Result<(), crate::error::ConfigError> {
        if self.global_hotkey.trim().is_empty() {
            return Err(crate::error::ConfigError::Invalid(
                "hotkey_empty".to_string(),
            ));
        }

        if self.transcription_provider.trim().is_empty() {
            return Err(crate::error::ConfigError::Invalid(
                "provider_empty".to_string(),
            ));
        }

        if !(200..=10_000).contains(&self.silence_timeout_ms) {
            return Err(crate::error::ConfigError::Invalid(
                "silence_timeout_range".to_string(),
            ));
        }

        if self.enter_trigger_phrase.chars().count() > 120 {
            return Err(crate::error::ConfigError::Invalid(
                "enter_trigger_phrase_too_long".to_string(),
            ));
        }

        if !(1..=5).contains(&self.local_whisper_beam_size) {
            return Err(crate::error::ConfigError::Invalid(
                "beam_size_range".to_string(),
            ));
        }

        if !Self::idle_unload_sec_valid(self.stt_idle_unload_sec) {
            return Err(crate::error::ConfigError::Invalid(
                "stt_idle_unload_range".to_string(),
            ));
        }

        if !Self::idle_unload_sec_valid(self.llm_idle_unload_sec) {
            return Err(crate::error::ConfigError::Invalid(
                "llm_idle_unload_range".to_string(),
            ));
        }

        if !(1..=8).contains(&self.weak_pc_ram_segment_cap) {
            return Err(crate::error::ConfigError::Invalid(
                "weak_pc_ram_segment_cap_range".to_string(),
            ));
        }

        if !(50..=4_096).contains(&self.weak_pc_max_disk_queue_mb) {
            return Err(crate::error::ConfigError::Invalid(
                "weak_pc_max_disk_queue_mb_range".to_string(),
            ));
        }

        if !(50..=2_000).contains(&self.vad_pre_speech_buffer_ms) {
            return Err(crate::error::ConfigError::Invalid(
                "vad_pre_speech_range".to_string(),
            ));
        }

        if !(50..=2_000).contains(&self.vad_minimum_speech_ms) {
            return Err(crate::error::ConfigError::Invalid(
                "vad_minimum_speech_range".to_string(),
            ));
        }

        if !(1_000..=120_000).contains(&self.vad_maximum_segment_ms) {
            return Err(crate::error::ConfigError::Invalid(
                "vad_maximum_segment_range".to_string(),
            ));
        }

        if !(3..=80).contains(&self.vad_voice_threshold_percent) {
            return Err(crate::error::ConfigError::Invalid(
                "vad_voice_threshold_range".to_string(),
            ));
        }

        if !(3..=80).contains(&self.vad_auto_threshold_percent) {
            return Err(crate::error::ConfigError::Invalid(
                "vad_auto_threshold_range".to_string(),
            ));
        }

        if self.text_processing_mode.requires_custom_skill()
            && self
                .ai_rewrite_skill
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(crate::error::ConfigError::Invalid(
                "custom_skill_missing".to_string(),
            ));
        }

        if let Some(language) = self.language.as_deref() {
            if !crate::transcription::languages::is_supported_transcription_language(language) {
                return Err(crate::error::ConfigError::Invalid(
                    "language_unsupported".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn vad_config(&self) -> crate::vad::VadConfig {
        crate::vad::VadConfig {
            engine: effective_vad_engine(self.vad_engine),
            pre_speech_buffer_ms: self.vad_pre_speech_buffer_ms,
            minimum_speech_ms: self.vad_minimum_speech_ms,
            silence_timeout_ms: self.silence_timeout_ms,
            maximum_segment_ms: self.vad_maximum_segment_ms,
            threshold_mode: self.vad_threshold_mode,
            voice_threshold_percent: self.vad_voice_threshold_percent,
            auto_threshold_percent: self.vad_auto_threshold_percent,
        }
    }

    pub fn effective_vad_threshold_percent(&self) -> u8 {
        self.vad_config().effective_threshold_percent()
    }

    pub fn effective_language(&self) -> String {
        self.language.clone().unwrap_or_else(|| match self.ui_locale {
            UiLocale::Ru => "ru".to_string(),
            UiLocale::En => "en".to_string(),
        })
    }

    /// Language for post-STT steps (e.g. numbers-as-words) when UI language is set to auto.
    pub fn postprocess_language(&self, whisper_detected: Option<&str>) -> String {
        if let Some(language) = &self.language {
            return language.clone();
        }
        whisper_detected
            .map(str::to_string)
            .unwrap_or_else(|| self.effective_language())
    }

    pub fn uses_openai_transcription(&self) -> bool {
        self.transcription_provider != "local"
    }

    pub fn needs_api_key(&self) -> bool {
        self.uses_openai_transcription()
            || matches!(self.text_rewrite_provider, TextRewriteProvider::Openai)
    }

    pub fn requires_api_key_for_transcription(&self) -> bool {
        self.uses_openai_transcription()
    }

    pub fn is_homemaker(&self) -> bool {
        matches!(self.ui_mode, UiMode::Homemaker)
    }

    /// Stored setting for light cleanup: «Оригинал» in standard UI, «Базовая очистка» in expert.
    pub fn canonical_light_cleanup_mode(&self) -> TextProcessingMode {
        if self.is_homemaker() {
            TextProcessingMode::Original
        } else {
            TextProcessingMode::Basic
        }
    }

    /// Standard (homemaker) UI stores «Оригинал» as [`TextProcessingMode::Original`]
    /// but applies basic cleanup during transcription processing.
    pub fn effective_text_processing_mode(&self) -> TextProcessingMode {
        if self.is_homemaker() && self.text_processing_mode == TextProcessingMode::Original {
            TextProcessingMode::Basic
        } else {
            self.text_processing_mode
        }
    }

    /// AI rewrite mode when two-phase PTT post-processing applies.
    pub fn ai_postprocess_mode(&self) -> Option<TextProcessingMode> {
        if !self.push_to_talk {
            return None;
        }
        let mode = self.text_processing_mode;
        mode.uses_ai().then_some(mode)
    }

    /// Phase-1 transcription cleanup (no AI rewrite in this step).
    pub fn immediate_transcription_mode(&self) -> TextProcessingMode {
        if self.ai_postprocess_mode().is_some() {
            // Two-phase PTT+AI: always inject «быстрая правка» (Basic) before the LLM pass.
            TextProcessingMode::Basic
        } else {
            self.effective_text_processing_mode()
        }
    }

    pub fn uses_cloud_storage(&self) -> bool {
        self.uses_openai_transcription()
            && matches!(self.text_rewrite_provider, TextRewriteProvider::Openai)
    }

    pub fn uses_local_storage(&self) -> bool {
        self.transcription_provider == "local"
            && matches!(self.text_rewrite_provider, TextRewriteProvider::Local)
    }

    pub fn weak_pc_active(&self) -> bool {
        self.weak_pc_mode
    }

    pub fn effective_spill_enabled(&self) -> bool {
        self.weak_pc_mode && self.weak_pc_spill_to_disk
    }

    pub fn should_reduce_prewarm_when_backlogged(&self) -> bool {
        self.weak_pc_mode && self.weak_pc_reduce_prewarm
    }

    fn idle_unload_sec_valid(sec: u32) -> bool {
        sec == 0 || (60..=7_200).contains(&sec)
    }
}

fn idle_unload_duration(sec: u32) -> Option<std::time::Duration> {
    if sec == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(sec as u64))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub enabled: Option<bool>,
    pub global_hotkey: Option<String>,
    pub capslock_ptt: Option<bool>,
    pub push_to_talk: Option<bool>,
    pub ptt_hold: Option<bool>,
    pub recording_indicator: Option<bool>,
    pub abort_on_focus_loss: Option<bool>,
    #[serde(alias = "live_dictation_field_indicator")]
    pub live_dictation_field_indicator: Option<bool>,
    pub hotkey_game_mode: Option<bool>,
    pub hotkey_block_system: Option<bool>,
    pub microphone_device: Option<Option<String>>,
    pub language: Option<Option<String>>,
    pub transcription_provider: Option<String>,
    pub transcription_model: Option<String>,
    pub local_whisper_models_dir: Option<Option<String>>,
    pub local_stt_family: Option<crate::settings::LocalSttFamily>,
    pub local_stt_quant: Option<crate::settings::LocalSttQuant>,
    #[serde(default, alias = "local_whisper_model")]
    pub local_stt_model: Option<crate::settings::LocalSttModelKind>,
    pub local_whisper_use_gpu: Option<bool>,
    pub local_whisper_beam_size: Option<u8>,
    pub local_sherpa_num_threads: Option<u32>,
    pub text_rewrite_provider: Option<TextRewriteProvider>,
    pub local_llm_model: Option<LlmModelKind>,
    pub local_llm_models_dir: Option<Option<String>>,
    pub local_llm_use_gpu: Option<bool>,
    pub ai_rewrite_skill: Option<Option<String>>,
    pub whisper_prompt_prefix: Option<String>,
    pub data_storage_dir: Option<Option<String>>,
    pub transcription_dictionary_path: Option<Option<String>>,
    pub audio_preprocess_enabled: Option<bool>,
    pub audio_noise_reduction_enabled: Option<bool>,
    pub vad_pre_speech_buffer_ms: Option<u32>,
    pub vad_minimum_speech_ms: Option<u32>,
    pub vad_maximum_segment_ms: Option<u32>,
    pub injection_mode: Option<InjectionMode>,
    pub spoken_punctuation: Option<bool>,
    #[serde(alias = "auto_punctuation_from_pauses")]
    pub silero_te: Option<bool>,
    pub text_processing_mode: Option<TextProcessingMode>,
    pub numbers_as_words: Option<bool>,
    pub emulate_enter: Option<bool>,
    pub enter_trigger_phrase: Option<String>,
    pub start_on_boot: Option<bool>,
    pub check_updates_on_startup: Option<bool>,
    pub show_notifications: Option<bool>,
    pub log_level: Option<String>,
    pub silence_timeout_ms: Option<u32>,
    pub vad_engine: Option<VadEngine>,
    pub vad_threshold_mode: Option<VadThresholdMode>,
    pub vad_voice_threshold_percent: Option<u8>,
    pub vad_auto_threshold_percent: Option<u8>,
    pub ui_locale: Option<UiLocale>,
    pub ui_mode: Option<UiMode>,
    pub homemaker_data_storage: Option<HomemakerDataStorage>,
    pub apply_homemaker_local_setup: Option<bool>,
    pub stt_idle_unload_sec: Option<u32>,
    pub llm_idle_unload_sec: Option<u32>,
    pub prewarm_local_models_at_startup: Option<bool>,
    pub weak_pc_mode: Option<bool>,
    pub weak_pc_spill_to_disk: Option<bool>,
    pub weak_pc_ram_segment_cap: Option<u32>,
    pub weak_pc_max_disk_queue_mb: Option<u32>,
    pub weak_pc_reduce_prewarm: Option<bool>,
}

impl SettingsPatch {
    pub fn apply_to(self, settings: &mut AppSettings) {
        if let Some(_enabled) = self.enabled {
            // Voice input is always active; persisted flag stays true.
        }
        settings.enabled = true;
        if let Some(global_hotkey) = self.global_hotkey {
            settings.global_hotkey = global_hotkey;
        }
        if let Some(push_to_talk) = self.push_to_talk {
            settings.push_to_talk = push_to_talk;
        }
        if let Some(ptt_hold) = self.ptt_hold {
            settings.ptt_hold = ptt_hold;
        }
        if let Some(recording_indicator) = self.recording_indicator {
            settings.recording_indicator = recording_indicator;
        } else if let Some(live_dictation_field_indicator) = self.live_dictation_field_indicator {
            settings.recording_indicator = live_dictation_field_indicator;
        }
        if let Some(abort_on_focus_loss) = self.abort_on_focus_loss {
            settings.abort_on_focus_loss = abort_on_focus_loss;
        }
        if let Some(hotkey_game_mode) = self.hotkey_game_mode {
            settings.hotkey_game_mode = hotkey_game_mode;
        }
        if let Some(hotkey_block_system) = self.hotkey_block_system {
            settings.hotkey_block_system = hotkey_block_system;
        }
        if let Some(microphone_device) = self.microphone_device {
            settings.microphone_device = microphone_device;
        }
        if let Some(language) = self.language {
            settings.language = language;
        }
        if let Some(transcription_provider) = self.transcription_provider {
            settings.transcription_provider = transcription_provider;
        }
        if let Some(transcription_model) = self.transcription_model {
            settings.transcription_model = transcription_model;
        }
        if let Some(local_whisper_models_dir) = self.local_whisper_models_dir {
            settings.local_whisper_models_dir = local_whisper_models_dir;
        }
        if let Some(local_stt_model) = self.local_stt_model {
            settings.set_local_stt_variant(crate::settings::migrate_from_legacy_stt_model(
                local_stt_model,
            ));
        } else {
            if let Some(local_stt_family) = self.local_stt_family {
                settings.local_stt_family = local_stt_family;
            }
            if let Some(local_stt_quant) = self.local_stt_quant {
                settings.local_stt_quant = local_stt_quant;
            }
            if self.local_stt_family.is_some() || self.local_stt_quant.is_some() {
                settings.sync_local_stt_model_from_variant();
            }
        }
        if let Some(local_whisper_use_gpu) = self.local_whisper_use_gpu {
            settings.local_whisper_use_gpu = local_whisper_use_gpu;
        }
        if let Some(local_whisper_beam_size) = self.local_whisper_beam_size {
            settings.local_whisper_beam_size = local_whisper_beam_size;
        }
        if let Some(local_sherpa_num_threads) = self.local_sherpa_num_threads {
            settings.local_sherpa_num_threads = local_sherpa_num_threads.clamp(1, 16);
        }
        if let Some(text_rewrite_provider) = self.text_rewrite_provider {
            settings.text_rewrite_provider = text_rewrite_provider;
        }
        if let Some(local_llm_model) = self.local_llm_model {
            settings.local_llm_model = local_llm_model;
        }
        if let Some(local_llm_models_dir) = self.local_llm_models_dir {
            settings.local_llm_models_dir = local_llm_models_dir;
        }
        if let Some(local_llm_use_gpu) = self.local_llm_use_gpu {
            settings.local_llm_use_gpu = local_llm_use_gpu;
        }
        if let Some(ai_rewrite_skill) = self.ai_rewrite_skill {
            settings.ai_rewrite_skill = ai_rewrite_skill;
        }
        if let Some(whisper_prompt_prefix) = self.whisper_prompt_prefix {
            settings.whisper_prompt_prefix = whisper_prompt_prefix;
        }
        if let Some(data_storage_dir) = self.data_storage_dir {
            settings.data_storage_dir = data_storage_dir;
            if settings.data_storage_dir.is_some() {
                settings.local_whisper_models_dir = None;
                settings.local_llm_models_dir = None;
                settings.transcription_dictionary_path = None;
            }
        }
        if let Some(transcription_dictionary_path) = self.transcription_dictionary_path {
            settings.transcription_dictionary_path = transcription_dictionary_path;
        }
        if let Some(audio_preprocess_enabled) = self.audio_preprocess_enabled {
            settings.audio_preprocess_enabled = audio_preprocess_enabled;
        }
        if let Some(audio_noise_reduction_enabled) = self.audio_noise_reduction_enabled {
            settings.audio_noise_reduction_enabled = audio_noise_reduction_enabled;
        }
        if let Some(vad_pre_speech_buffer_ms) = self.vad_pre_speech_buffer_ms {
            settings.vad_pre_speech_buffer_ms = vad_pre_speech_buffer_ms;
        }
        if let Some(vad_minimum_speech_ms) = self.vad_minimum_speech_ms {
            settings.vad_minimum_speech_ms = vad_minimum_speech_ms;
        }
        if let Some(vad_maximum_segment_ms) = self.vad_maximum_segment_ms {
            settings.vad_maximum_segment_ms = vad_maximum_segment_ms;
        }
        if let Some(injection_mode) = self.injection_mode {
            settings.injection_mode = injection_mode;
        }
        if let Some(spoken_punctuation) = self.spoken_punctuation {
            settings.spoken_punctuation = spoken_punctuation;
        }
        if let Some(silero_te) = self.silero_te {
            settings.silero_te = silero_te;
        }
        if let Some(text_processing_mode) = self.text_processing_mode {
            settings.text_processing_mode = text_processing_mode;
        }
        if let Some(numbers_as_words) = self.numbers_as_words {
            settings.numbers_as_words = numbers_as_words;
        }
        if let Some(emulate_enter) = self.emulate_enter {
            settings.emulate_enter = emulate_enter;
        }
        if let Some(enter_trigger_phrase) = self.enter_trigger_phrase {
            settings.enter_trigger_phrase = enter_trigger_phrase;
        }
        if let Some(start_on_boot) = self.start_on_boot {
            settings.start_on_boot = start_on_boot;
        }
        if let Some(check_updates_on_startup) = self.check_updates_on_startup {
            settings.check_updates_on_startup = check_updates_on_startup;
        }
        if let Some(capslock_ptt) = self.capslock_ptt {
            settings.capslock_ptt = capslock_ptt;
        }
        if let Some(show_notifications) = self.show_notifications {
            settings.show_notifications = show_notifications
                && crate::notify::system_notifications_available();
        }
        if let Some(log_level) = self.log_level {
            settings.log_level = log_level;
        }
        if let Some(silence_timeout_ms) = self.silence_timeout_ms {
            settings.silence_timeout_ms = silence_timeout_ms;
        }
        if let Some(vad_engine) = self.vad_engine {
            settings.vad_engine = vad_engine;
        }
        if let Some(vad_threshold_mode) = self.vad_threshold_mode {
            settings.vad_threshold_mode = vad_threshold_mode;
        }
        if let Some(vad_voice_threshold_percent) = self.vad_voice_threshold_percent {
            settings.vad_voice_threshold_percent = vad_voice_threshold_percent;
        }
        if let Some(vad_auto_threshold_percent) = self.vad_auto_threshold_percent {
            settings.vad_auto_threshold_percent = vad_auto_threshold_percent;
        }
        if let Some(ui_locale) = self.ui_locale {
            settings.ui_locale = ui_locale;
        }
        if let Some(ui_mode) = self.ui_mode {
            settings.ui_mode = ui_mode;
        }
        if let Some(homemaker_data_storage) = self.homemaker_data_storage {
            match homemaker_data_storage {
                HomemakerDataStorage::Cloud => {
                    settings.transcription_provider = "openai".to_string();
                    settings.text_rewrite_provider = TextRewriteProvider::Openai;
                }
                HomemakerDataStorage::Local => {
                    settings.transcription_provider = "local".to_string();
                    settings.text_rewrite_provider = TextRewriteProvider::Local;
                }
            }
        }
        if let Some(stt_idle_unload_sec) = self.stt_idle_unload_sec {
            settings.stt_idle_unload_sec = stt_idle_unload_sec;
        }
        if let Some(llm_idle_unload_sec) = self.llm_idle_unload_sec {
            settings.llm_idle_unload_sec = llm_idle_unload_sec;
        }
        if let Some(prewarm_local_models_at_startup) = self.prewarm_local_models_at_startup {
            settings.prewarm_local_models_at_startup = prewarm_local_models_at_startup;
        }
        if let Some(weak_pc_mode) = self.weak_pc_mode {
            settings.weak_pc_mode = weak_pc_mode;
        }
        if let Some(weak_pc_spill_to_disk) = self.weak_pc_spill_to_disk {
            settings.weak_pc_spill_to_disk = weak_pc_spill_to_disk;
        }
        if let Some(weak_pc_ram_segment_cap) = self.weak_pc_ram_segment_cap {
            settings.weak_pc_ram_segment_cap = weak_pc_ram_segment_cap;
        }
        if let Some(weak_pc_max_disk_queue_mb) = self.weak_pc_max_disk_queue_mb {
            settings.weak_pc_max_disk_queue_mb = weak_pc_max_disk_queue_mb;
        }
        if let Some(weak_pc_reduce_prewarm) = self.weak_pc_reduce_prewarm {
            settings.weak_pc_reduce_prewarm = weak_pc_reduce_prewarm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_valid() {
        AppSettings::default().validate().unwrap();
    }

    #[test]
    fn vad_config_uses_silence_timeout_from_settings() {
        let settings = AppSettings {
            silence_timeout_ms: 1200,
            ..Default::default()
        };
        assert_eq!(settings.vad_config().silence_timeout_ms, 1200);
    }

    #[test]
    fn patch_applies_partial_updates() {
        let mut settings = AppSettings::default();
        SettingsPatch {
            enabled: Some(true),
            language: Some(Some("ru".to_string())),
            ..Default::default()
        }
        .apply_to(&mut settings);

        assert!(settings.enabled);
        assert_eq!(settings.language.as_deref(), Some("ru"));
        assert_eq!(settings.global_hotkey, "Shift+F1");
    }

    #[test]
    fn ai_modes_are_detected() {
        assert!(TextProcessingMode::Optimization.uses_ai());
        assert!(TextProcessingMode::CustomSkill.uses_ai());
        assert!(!TextProcessingMode::Basic.uses_ai());
    }

    #[test]
    fn ai_postprocess_requires_ptt_and_ai_mode() {
        let mut settings = AppSettings {
            push_to_talk: true,
            text_processing_mode: TextProcessingMode::Optimization,
            ..Default::default()
        };
        assert_eq!(
            settings.ai_postprocess_mode(),
            Some(TextProcessingMode::Optimization)
        );

        settings.push_to_talk = false;
        assert_eq!(settings.ai_postprocess_mode(), None);

        settings.push_to_talk = true;
        settings.text_processing_mode = TextProcessingMode::Basic;
        assert_eq!(settings.ai_postprocess_mode(), None);
    }

    #[test]
    fn immediate_transcription_mode_for_ptt_ai() {
        let expert = AppSettings {
            push_to_talk: true,
            text_processing_mode: TextProcessingMode::Optimization,
            ui_mode: UiMode::Expert,
            ..Default::default()
        };
        assert_eq!(
            expert.immediate_transcription_mode(),
            TextProcessingMode::Basic
        );

        let homemaker = AppSettings {
            push_to_talk: true,
            text_processing_mode: TextProcessingMode::Optimization,
            ui_mode: UiMode::Homemaker,
            ..Default::default()
        };
        assert_eq!(
            homemaker.immediate_transcription_mode(),
            TextProcessingMode::Basic
        );

        let continuous = AppSettings {
            push_to_talk: false,
            text_processing_mode: TextProcessingMode::Optimization,
            ..Default::default()
        };
        assert_eq!(
            continuous.immediate_transcription_mode(),
            TextProcessingMode::Optimization
        );
    }

    #[test]
    fn russian_llm_model_visible_only_for_ru_ui() {
        assert!(LlmModelKind::TLiteIt21.visible_for_ui_locale(UiLocale::Ru));
        assert!(!LlmModelKind::TLiteIt21.visible_for_ui_locale(UiLocale::En));
        assert!(LlmModelKind::Qwen3_4B.visible_for_ui_locale(UiLocale::En));
    }

    #[test]
    fn whisper_model_urls_use_ggerganov_repo() {
        assert_eq!(
            WhisperModelKind::Base.file_name(),
            "ggml-base.bin"
        );
        assert!(WhisperModelKind::Small
            .download_url()
            .ends_with("ggml-small.bin"));
        assert!(WhisperModelKind::LargeV3Turbo
            .download_url()
            .ends_with("ggml-large-v3-turbo.bin"));
        assert!(WhisperModelKind::LargeV3
            .download_url()
            .ends_with("ggml-large-v3.bin"));
    }

    #[test]
    fn whisper_model_kind_deserializes_from_frontend_values() {
        for (value, expected) in [
            ("base", WhisperModelKind::Base),
            ("small", WhisperModelKind::Small),
            ("medium", WhisperModelKind::Medium),
            ("large_v3_turbo", WhisperModelKind::LargeV3Turbo),
            ("large_v3", WhisperModelKind::LargeV3),
        ] {
            let parsed: WhisperModelKind = serde_json::from_str(&format!("\"{value}\"")).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn custom_skill_mode_requires_skill_file() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::CustomSkill,
            ai_rewrite_skill: None,
            ..Default::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn local_transcription_does_not_require_api_key() {
        let settings = AppSettings {
            transcription_provider: "local".to_string(),
            text_processing_mode: TextProcessingMode::Optimization,
            ..Default::default()
        };
        assert!(!settings.requires_api_key_for_transcription());
        assert!(settings.needs_api_key());
    }

    #[test]
    fn local_rewrite_provider_does_not_need_api_key() {
        let settings = AppSettings {
            transcription_provider: "local".to_string(),
            text_processing_mode: TextProcessingMode::Optimization,
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(!settings.needs_api_key());
    }

    #[test]
    fn gec_model_is_detected() {
        assert!(LlmModelKind::Gec08B.is_gec());
        assert!(!LlmModelKind::Qwen3_4B.is_gec());
    }

    #[test]
    fn llm_model_kind_uses_stable_api_names() {
        for kind in LlmModelKind::all() {
            let json = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            let parsed: LlmModelKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn capslock_ptt_switches_hotkey_and_restores_it() {
        let mut settings = AppSettings {
            global_hotkey: "Alt+Cmd+KeyZ".to_string(),
            ..Default::default()
        };
        settings.capslock_ptt = true;
        settings.sync_capslock_hotkey(false);
        assert_eq!(settings.global_hotkey, "F18");
        assert_eq!(
            settings.hotkey_before_capslock.as_deref(),
            Some("Alt+Cmd+KeyZ")
        );

        settings.capslock_ptt = false;
        settings.sync_capslock_hotkey(true);
        assert_eq!(settings.global_hotkey, "Alt+Cmd+KeyZ");
        assert_eq!(settings.hotkey_before_capslock, None);
    }

    #[test]
    fn turning_capslock_ptt_off_keeps_a_hotkey_picked_meanwhile() {
        let mut settings = AppSettings {
            capslock_ptt: true,
            global_hotkey: "Shift+F2".to_string(),
            hotkey_before_capslock: Some("Alt+Cmd+KeyZ".to_string()),
            ..Default::default()
        };
        settings.capslock_ptt = false;
        settings.sync_capslock_hotkey(true);
        assert_eq!(settings.global_hotkey, "Shift+F2");
        assert_eq!(settings.hotkey_before_capslock, None);
    }

    #[test]
    fn unchanged_capslock_ptt_leaves_hotkey_alone() {
        let mut settings = AppSettings {
            global_hotkey: "Shift+F2".to_string(),
            ..Default::default()
        };
        settings.sync_capslock_hotkey(false);
        assert_eq!(settings.global_hotkey, "Shift+F2");
        assert_eq!(settings.hotkey_before_capslock, None);
    }

    #[test]
    fn continuous_mode_preserves_game_mode_settings() {
        let mut settings = AppSettings::default();
        settings.push_to_talk = false;
        settings.hotkey_game_mode = true;
        settings.hotkey_block_system = true;
        settings.validate().expect("continuous game mode settings");
        assert!(settings.hotkey_game_mode);
        assert!(settings.hotkey_block_system);
    }

    #[test]
    fn vad_threshold_settings_validate_and_map_to_config() {
        let mut settings = AppSettings::default();
        settings.vad_threshold_mode = VadThresholdMode::Manual;
        settings.vad_voice_threshold_percent = 22;
        settings.vad_auto_threshold_percent = 14;
        settings.validate().expect("vad threshold settings");

        let config = settings.vad_config();
        assert_eq!(config.threshold_mode, VadThresholdMode::Manual);
        assert_eq!(config.voice_threshold_percent, 22);
        assert_eq!(config.effective_threshold_percent(), 22);
    }
}
