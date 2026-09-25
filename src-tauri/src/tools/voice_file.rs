use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tracing::{info, warn};

use crate::app::context::AppContext;
use crate::app::events::{emit_voice_file_progress, VoiceFileProgressPayload, VoiceFileProgressPhase};
use crate::app::transcribe_audio::transcribe_segment_with_retries;
use crate::audio::decode_file::{decode_audio_file_with_progress, DecodeProgressCallback};
use crate::transcription::models::WhisperProgressCallback;
use crate::audio::preprocess::{preprocess_segment, PreprocessOptions};
use crate::error::AppError;
use crate::llm::LlmEngine;
use crate::settings::AppSettings;
use crate::text::pipeline::{process_transcription, ProcessTranscriptionFlags};
use crate::transcription::prompt::WhisperPromptInput;

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
}

struct ToolsTranscriptionGuard<'a> {
    ctx: &'a AppContext,
    active: bool,
}

impl<'a> ToolsTranscriptionGuard<'a> {
    fn try_begin(ctx: &'a AppContext) -> Result<Self, String> {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        if controller.blocks_tools_transcription() {
            return Err("tools.voiceFiles.pttBusy".to_string());
        }
        if controller.is_tools_transcription_busy() {
            return Err("tools.voiceFiles.busy".to_string());
        }
        controller.set_tools_transcription_busy(true);
        Ok(Self {
            ctx,
            active: true,
        })
    }
}

impl Drop for ToolsTranscriptionGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut controller) = self.ctx.controller.lock() {
            controller.set_tools_transcription_busy(false);
        }
    }
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
    let last = Arc::new(AtomicU8::new(0));
    Arc::new(move |percent: u8| {
        let prev = last.load(Ordering::Relaxed);
        if percent < prev.saturating_add(2) && percent < 100 {
            return;
        }
        last.store(percent, Ordering::Relaxed);
        emit_progress(&app, &path, phase, Some(percent));
    })
}

pub async fn transcribe_voice_file(
    app: AppHandle,
    ctx: Arc<AppContext>,
    path: String,
) -> Result<VoiceFileTranscriptionResult, String> {
    let _guard = ToolsTranscriptionGuard::try_begin(&ctx)?;

    let path_key = path.trim().to_string();
    if path_key.is_empty() {
        return Err("tools.voiceFiles.invalidPath".to_string());
    }

    let path = PathBuf::from(&path_key);

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    if file_name.is_empty() {
        return Err("tools.voiceFiles.invalidPath".to_string());
    }

    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    let ai_mode = settings.effective_text_processing_mode();

    emit_phase(&app, &path_key, VoiceFileProgressPhase::Decoding);
    let decode_progress: DecodeProgressCallback =
        throttled_phase_progress(app.clone(), path_key.clone(), VoiceFileProgressPhase::Decoding);
    let segment = decode_audio_file_with_progress(&path, Some(decode_progress))
        .map_err(map_decode_error)?;

    let captured = segment.clone();
    let preprocessed = preprocess_segment(
        captured.clone(),
        PreprocessOptions {
            enabled: settings.audio_preprocess_enabled,
            noise_reduction_enabled: settings.audio_preprocess_enabled
                && settings.audio_noise_reduction_enabled,
            normalize_only: false,
        },
    );
    if preprocessed.skipped_as_silence {
        emit_phase(&app, &path_key, VoiceFileProgressPhase::Done);
        return Ok(VoiceFileTranscriptionResult {
            file_name,
            text: String::new(),
            rewrite_fallback: false,
            rewrite_fallback_reason: None,
            ai_rewrite_applied: false,
            gec_grammar_only: false,
        });
    }
    let segment = preprocessed.segment;

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
    )
    .await
    .map_err(|error| error.user_message(settings.ui_locale))?;

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

    Ok(VoiceFileTranscriptionResult {
        file_name,
        text: processed.text,
        rewrite_fallback: processed.rewrite_fallback,
        rewrite_fallback_reason: processed.rewrite_fallback_reason,
        ai_rewrite_applied,
        gec_grammar_only,
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
        },
    )
    .await
    .map_err(|error| error.to_string())
}

fn map_decode_error(error: String) -> String {
    if error == "empty_audio" {
        return "tools.voiceFiles.emptyAudio".to_string();
    }
    if error.contains("unsupported codec") || error.contains("unsupported feature") {
        return "tools.voiceFiles.unsupportedFormat".to_string();
    }
    warn!("voice file decode failed: {error}");
    return "tools.voiceFiles.readFailed".to_string();
}
