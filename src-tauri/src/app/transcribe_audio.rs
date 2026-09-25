use std::sync::Arc;

use crate::transcription::models::WhisperProgressCallback;

use tracing::warn;

use crate::audio::preprocess::{preprocess_segment, PreprocessOptions};
use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
use crate::transcription::prompt::{build_whisper_prompt, WhisperPromptInput};
use crate::transcription::{
    TranscriptionError, TranscriptionOptions, TranscriptionProvider, TranscriptionResult,
    WhisperDecodingOptions,
};

const RETRY_MIN_DURATION_MS: u64 = 400;

pub async fn transcribe_segment_with_retries(
    transcriber: Arc<dyn TranscriptionProvider>,
    segment: AudioSegment,
    captured: AudioSegment,
    settings: &AppSettings,
    prompt_input: WhisperPromptInput<'_>,
    whisper_progress: Option<WhisperProgressCallback>,
) -> Result<TranscriptionResult, TranscriptionError> {
    let prompt = build_whisper_prompt(&prompt_input);
    let options = TranscriptionOptions {
        language: settings.language.clone(),
        prompt: prompt.clone(),
        model: settings.transcription_model.clone(),
        whisper_decoding: Some(WhisperDecodingOptions::default()),
        dictionary_path: crate::settings::resolve_dictionary_file_path(settings)
            .ok()
            .map(|path| path.display().to_string()),
        whisper_progress: whisper_progress.clone(),
    };

    let mut transcription = transcriber
        .transcribe(segment.clone(), options.clone())
        .await;

    if matches!(transcription.as_ref(), Ok(result) if result.text.is_empty())
        && captured.duration_ms >= RETRY_MIN_DURATION_MS
    {
        let normalize_only = preprocess_segment(
            captured.clone(),
            PreprocessOptions {
                enabled: true,
                noise_reduction_enabled: false,
                normalize_only: true,
            },
        );
        if !normalize_only.skipped_as_silence {
            warn!(
                duration_ms = captured.duration_ms,
                "retrying transcription with normalize-only preprocess"
            );
            transcription = transcriber
                .transcribe(normalize_only.segment, options.clone())
                .await;
        }
    }

    if matches!(transcription.as_ref(), Ok(result) if result.text.is_empty())
        && captured.duration_ms >= RETRY_MIN_DURATION_MS
    {
        warn!(
            duration_ms = captured.duration_ms,
            "retrying transcription on raw capture audio"
        );
        let permissive = TranscriptionOptions {
            whisper_decoding: Some(WhisperDecodingOptions::permissive()),
            prompt: None,
            whisper_progress: whisper_progress.clone(),
            ..options
        };
        transcription = transcriber
            .transcribe(captured.clone(), permissive)
            .await;
    }

    transcription
}
