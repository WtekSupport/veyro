pub mod audio_to_srt;
pub mod audio_to_srt_timing;
pub mod dictation_transcripts;
pub mod docx_export;
pub mod shared;
pub mod voice_file;
pub mod voice_watch;
#[cfg(feature = "local-separation")]
pub mod vocal_separator;

pub use audio_to_srt::{
    subtitle_stt_capability, transcribe_audio_to_srt, AudioToSrtOptions, AudioToSrtResult,
    SubtitleSttCapability,
};
pub use voice_file::{
    transcribe_voice_file, VoiceFileOptions, VoiceFileTranscriptionResult,
};

#[cfg(feature = "local-separation")]
pub use vocal_separator::{
    separate_vocal_file, split_instrumental_further, SplitInstrumentalResult,
    VocalSeparatorInvokeOptions, VocalSeparatorResult,
};

#[cfg(not(feature = "local-separation"))]
mod vocal_separator_stub {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VocalSeparatorInvokeOptions {
        #[serde(default)]
        pub profile: Option<crate::settings::VocalSeparatorProfile>,
        #[serde(default)]
        pub output_format: Option<crate::settings::VocalSeparatorOutputFormat>,
        #[serde(default)]
        pub output_dir: Option<String>,
        #[serde(default)]
        pub normalize: Option<bool>,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VocalSeparatorResult {
        pub file_name: String,
        pub vocals_path: String,
        pub instrumental_path: String,
        #[serde(default)]
        pub warnings: Vec<String>,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InstrumentStemResult {
        pub name: String,
        pub path: String,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SplitInstrumentalResult {
        pub file_name: String,
        pub stems: Vec<InstrumentStemResult>,
        #[serde(default)]
        pub warnings: Vec<String>,
    }

    pub async fn split_instrumental_further(
        _app: tauri::AppHandle,
        _ctx: std::sync::Arc<crate::app::context::AppContext>,
        _path: String,
        _options: VocalSeparatorInvokeOptions,
    ) -> Result<SplitInstrumentalResult, String> {
        Err("tools.vocalSeparator.unavailable".to_string())
    }
}

#[cfg(not(feature = "local-separation"))]
pub use vocal_separator_stub::{
    split_instrumental_further, SplitInstrumentalResult, VocalSeparatorInvokeOptions,
    VocalSeparatorResult,
};
