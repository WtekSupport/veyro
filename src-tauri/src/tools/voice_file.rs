use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tracing::info;

use crate::app::context::AppContext;
use crate::app::events::{emit_voice_file_progress, VoiceFileProgressPayload, VoiceFileProgressPhase};
use crate::app::transcribe_audio::{transcribe_segment_with_retries, TranscribeSegmentFlags};
use crate::transcription::models::WhisperProgressCallback;
use crate::error::AppError;
use crate::llm::LlmEngine;
use crate::settings::AppSettings;
use crate::text::pipeline::{process_transcription, ProcessTranscriptionFlags};
use crate::transcription::prompt::WhisperPromptInput;

use super::shared::{
    decode_and_preprocess_for_tools, stt_failed_in_auto_mode, throttled_percent_callback,
    ToolsTranscriptionGuard, STT_SELECT_LANGUAGE_ERROR, validate_tool_file_path,
};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceFileOptions {
    #[serde(default)]
    pub stt_language_override: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceFileTranscriptionResult {
    pub file_name: String,
    pub text: String,
    pub rewrite_fallback: bool,
    pub rewrite_fallback_reason: Option<String>,
    pub ai_rewrite_applied: bool,
    /// GEC local model in Optimization mode — grammar pass only, not full literary rewrite.
    pub gec_grammar_only: bool,
    /// When `text` is empty, UI may show a localized explanation (`tools.voiceFiles.*` key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info_message_key: Option<String>,
}

fn emit_phase(app: &AppHandle, path: &str, phase: VoiceFileProgressPhase) {
    emit_progress(app, path, phase, None);
}

fn emit_progress(
    app: &AppHandle,
    path: &str,
    phase: VoiceFileProgressPhase,
    percent: Option<u8>,
) {
    emit_voice_file_progress(
        app,
        VoiceFileProgressPayload {
            path: path.to_string(),
            phase,
            percent,
        },
    );
}

fn throttled_phase_progress(
    app: AppHandle,
    path: String,
    phase: VoiceFileProgressPhase,
) -> Arc<dyn Fn(u8) + Send + Sync + 'static> {
    throttled_percent_callback(Arc::new(move |percent: u8| {
        emit_progress(&app, &path, phase, Some(percent));
    }))
}

pub async fn transcribe_voice_file(
    app: AppHandle,
    ctx: Arc<AppContext>,
    path: String,
    options: VoiceFileOptions,
) -> Result<VoiceFileTranscriptionResult, String> {
    let _guard = ToolsTranscriptionGuard::try_begin(&ctx)?;

    let (path_key, path_buf, file_name) = validate_tool_file_path(&path, "tools.voiceFiles")?;

    let base_settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    let mut settings = base_settings.clone();
    if let Some(language) = options
        .stt_language_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.language = Some(language.to_string());
    }
    let stt_auto_mode =
        stt_failed_in_auto_mode(&base_settings, options.stt_language_override.as_deref());

    let ai_mode = settings.effective_text_processing_mode();

    emit_phase(&app, &path_key, VoiceFileProgressPhase::Decoding);
    let decode_progress: WhisperProgressCallback =
        throttled_phase_progress(app.clone(), path_key.clone(), VoiceFileProgressPhase::Decoding);
    let prepared = decode_and_preprocess_for_tools(
        &path_buf,
        &settings,
        Some(decode_progress),
        "tools.voiceFiles",
    )?;

    if prepared.skipped_as_silence {
        emit_phase(&app, &path_key, VoiceFileProgressPhase::Done);
        return Ok(VoiceFileTranscriptionResult {
            file_name,
            text: String::new(),
            rewrite_fallback: false,
            rewrite_fallback_reason: None,
            ai_rewrite_applied: false,
            gec_grammar_only: false,
            info_message_key: Some("tools.voiceFiles.silenceOnly".to_string()),
        });
    }
    let segment = prepared.segment;
    let captured = prepared.captured;

    let dictionary = crate::text::dictionary::load_dictionary_for_settings(&settings)
        .unwrap_or_default();
    let prompt_input = WhisperPromptInput {
        settings: &settings,
        vocabulary: &dictionary.vocabulary,
        previous_text: None,
    };

    emit_phase(&app, &path_key, VoiceFileProgressPhase::Transcribing);
    let transcribe_progress: WhisperProgressCallback = throttled_phase_progress(
        app.clone(),
        path_key.clone(),
        VoiceFileProgressPhase::Transcribing,
    );
    let transcriber = ctx.runtime.transcriber();
    let transcription = transcribe_segment_with_retries(
        transcriber,
        segment,
        captured,
        &settings,
        prompt_input,
        Some(transcribe_progress),
        TranscribeSegmentFlags::default(),
        None,
    )
    .await
    .map_err(|error| error.user_message(settings.ui_locale))?;

    if transcription.text.trim().is_empty() && stt_auto_mode {
        return Err(STT_SELECT_LANGUAGE_ERROR.to_string());
    }

    if crate::text::normalize::is_music_or_media_label_only(&transcription.text) {
        emit_phase(&app, &path_key, VoiceFileProgressPhase::Done);
        return Ok(VoiceFileTranscriptionResult {
            file_name,
            text: String::new(),
            rewrite_fallback: false,
            rewrite_fallback_reason: None,
            ai_rewrite_applied: false,
            gec_grammar_only: false,
            info_message_key: Some("tools.voiceFiles.musicOnly".to_string()),
        });
    }

    emit_phase(&app, &path_key, VoiceFileProgressPhase::TextCleanup);
    let processed = process_voice_file_text(
        &app,
        &ctx,
        &settings,
        &path_key,
        &transcription.text,
        transcription.detected_language.as_deref(),
    )
    .await?;

    emit_phase(&app, &path_key, VoiceFileProgressPhase::Done);

    let gec_grammar_only =
        ai_mode == crate::settings::TextProcessingMode::Optimization
            && settings.local_llm_model.is_gec()
            && matches!(
                settings.text_rewrite_provider,
                crate::settings::TextRewriteProvider::Local
            );
    let ai_rewrite_applied = ai_mode.uses_ai() && !processed.rewrite_fallback && !gec_grammar_only;

    let info_message_key = if processed.text.trim().is_empty()
        && crate::text::normalize::is_music_or_media_label_only(&transcription.text)
    {
        Some("tools.voiceFiles.musicOnly".to_string())
    } else {
        None
    };

    Ok(VoiceFileTranscriptionResult {
        file_name,
        text: processed.text,
        rewrite_fallback: processed.rewrite_fallback,
        rewrite_fallback_reason: processed.rewrite_fallback_reason,
        ai_rewrite_applied,
        gec_grammar_only,
        info_message_key,
    })
}

async fn process_voice_file_text(
    app: &AppHandle,
    ctx: &AppContext,
    settings: &AppSettings,
    path_key: &str,
    raw: &str,
    whisper_detected_language: Option<&str>,
) -> Result<crate::text::ProcessedText, String> {
    let ai_mode = settings.effective_text_processing_mode();
    if ai_mode.uses_ai() {
        emit_phase(app, path_key, VoiceFileProgressPhase::AiRewrite);
        info!(
            "voice file AI text rewrite ({})",
            ai_mode.as_str()
        );
    }

    if crate::llm::model_store::needs_local_llm(settings) {
        LlmEngine::ensure_loaded(settings, &ctx.llm_engine)
            .map_err(|error| AppError::Internal(error).to_string())?;
    }

    let llm_snapshot = ctx
        .llm_engine
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone());

    process_transcription(
        raw,
        None,
        settings,
        &ctx.http,
        &llm_snapshot,
        whisper_detected_language,
        ProcessTranscriptionFlags {
            force_ai_rewrite: true,
            ..Default::default()
        },
    )
    .await
    .map_err(|error| error.to_string())
}
