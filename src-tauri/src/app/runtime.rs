use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::app::activity_log::{ActivityLevel, ActivityLog};
use serde_json::{json, Value};
use crate::app::context::AppContext;
use crate::app::controller::{with_controller, SharedController};
use crate::app::events::{
    emit_activity_log, emit_error, emit_injection_completed, emit_listening_stopped,
    emit_transcription_completed, emit_transcription_started, TranscriptionCompletedPayload,
};
use crate::app::state::AppState;
use crate::audio::debug::save_last_ptt_wav;
use crate::audio::preprocess::{audio_peak_rms, preprocess_segment, PreprocessOptions};
use crate::audio::segment::AudioSegment;
use crate::error::AppError;
use crate::injection::TextInjector;
use crate::privacy::retention::clear_segment;
use crate::i18n;
use crate::settings::{AppSettings, TextRewriteProvider, UiLocale};
use crate::network::HttpClient;
use crate::llm::LlmEngine;
use crate::text::dictionary::load_dictionary;
use crate::text::normalize::ensure_trailing_block_separator;
use crate::text::process_transcription;
use crate::transcription::prompt::{build_whisper_prompt, WhisperPromptInput};
use crate::transcription::{
    create_transcriber, TranscriptionOptions, TranscriptionProvider, WhisperDecodingOptions,
};
use crate::tray;

struct SegmentJob {
    app: AppHandle,
    controller: SharedController,
    segment: AudioSegment,
    settings: AppSettings,
}

pub struct PipelineRuntime {
    transcriber: Arc<RwLock<Arc<dyn TranscriptionProvider>>>,
    llm_engine: Arc<RwLock<LlmEngine>>,
    injector: Arc<dyn TextInjector>,
    cancel: Arc<RwLock<CancellationToken>>,
    pending: Arc<AtomicUsize>,
    queue_tx: UnboundedSender<SegmentJob>,
    activity_log: Arc<ActivityLog>,
}

impl PipelineRuntime {
    pub fn new(
        transcriber: Arc<dyn TranscriptionProvider>,
        injector: Arc<dyn TextInjector>,
        http: reqwest::Client,
        cancel: CancellationToken,
        activity_log: Arc<ActivityLog>,
        llm_engine: LlmEngine,
    ) -> Self {
        let pending = Arc::new(AtomicUsize::new(0));
        let (queue_tx, mut queue_rx) = unbounded_channel();
        let transcriber = Arc::new(RwLock::new(transcriber));
        let llm_engine = Arc::new(RwLock::new(llm_engine));
        let cancel = Arc::new(RwLock::new(cancel));

        let worker_transcriber = transcriber.clone();
        let worker_llm_engine = llm_engine.clone();
        let worker_injector = injector.clone();
        let worker_http = Arc::new(RwLock::new(http));
        let worker_cancel = cancel.clone();
        let worker_pending = pending.clone();
        let worker_log = activity_log.clone();
        let worker_context = Arc::new(Mutex::new(String::new()));

        tauri::async_runtime::spawn(async move {
            while let Some(job) = queue_rx.recv().await {
                let http = worker_http
                    .read()
                    .map(|guard| (*guard).clone())
                    .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
                process_one_segment(
                    job,
                    worker_transcriber.clone(),
                    worker_injector.clone(),
                    http,
                    worker_http.clone(),
                    worker_cancel.clone(),
                    worker_pending.clone(),
                    worker_log.clone(),
                    worker_context.clone(),
                    worker_llm_engine.clone(),
                )
                .await;
            }
        });

        Self {
            transcriber,
            llm_engine,
            injector,
            cancel,
            pending,
            queue_tx,
            activity_log,
        }
    }

    pub fn set_transcriber(&self, transcriber: Arc<dyn TranscriptionProvider>) {
        if let Ok(mut guard) = self.transcriber.write() {
            *guard = transcriber;
        }
    }

    pub async fn unload_transcriber(&self) {
        let transcriber = self
            .transcriber
            .read()
            .map(|guard| Arc::clone(&*guard))
            .ok();
        if let Some(transcriber) = transcriber {
            tokio::spawn(async move {
                let _ = transcriber.unload().await;
            });
        }
    }

    pub fn is_whisper_model_loaded(&self) -> bool {
        self.transcriber
            .read()
            .map(|guard| guard.is_model_loaded())
            .unwrap_or(false)
    }

    pub fn set_llm_engine(&self, llm_engine: LlmEngine) {
        if let Ok(mut guard) = self.llm_engine.write() {
            *guard = llm_engine;
        }
    }

    pub async fn prewarm_transcriber(&self) -> Result<(), crate::transcription::TranscriptionError> {
        let transcriber = self
            .transcriber
            .read()
            .map(|guard| Arc::clone(&*guard))
            .map_err(|_| {
                crate::transcription::TranscriptionError::InferenceFailed(
                    "transcriber lock poisoned".to_string(),
                )
            })?;
        transcriber.prewarm().await
    }

    pub fn pending_count(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }

    pub fn transcriber(&self) -> Arc<dyn TranscriptionProvider> {
        self.transcriber
            .read()
            .map(|guard| Arc::clone(&*guard))
            .unwrap_or_else(|poisoned| Arc::clone(&*poisoned.into_inner()))
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    pub fn reset_cancel(&self) {
        if let Ok(mut guard) = self.cancel.write() {
            *guard = CancellationToken::new();
        }
    }

    pub fn cancel_pending(&self) {
        if let Ok(guard) = self.cancel.read() {
            guard.cancel();
        }
    }

    pub fn injector(&self) -> Arc<dyn TextInjector> {
        self.injector.clone()
    }

    pub fn process_segment(
        &self,
        app: AppHandle,
        controller: SharedController,
        segment: AudioSegment,
        settings: AppSettings,
    ) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        let queued = self.pending.load(Ordering::SeqCst);
        log_activity(
            &self.activity_log,
            Some(&app),
            ActivityLevel::Info,
            "activity.queue.enqueued",
            json!({ "ms": segment.duration_ms, "pending": queued }),
        );

        if self
            .queue_tx
            .send(SegmentJob {
                app,
                controller,
                segment,
                settings,
            })
            .is_err()
        {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn log_activity(
    activity_log: &ActivityLog,
    app: Option<&AppHandle>,
    level: ActivityLevel,
    message_key: &str,
    message_args: Value,
) {
    activity_log.push(level, message_key, message_args);
    if let Some(app) = app {
        emit_activity_log(app, &activity_log.snapshot());
    }
}

fn rebuild_http_client() -> reqwest::Client {
    HttpClient::new()
        .map(|client| client.inner().clone())
        .unwrap_or_else(|error| {
            warn!("failed to rebuild HTTP client: {error}");
            reqwest::Client::new()
        })
}

fn reload_stt_engine(
    transcriber: &Arc<RwLock<Arc<dyn TranscriptionProvider>>>,
    shared_http: &Arc<RwLock<reqwest::Client>>,
    settings: &AppSettings,
    cancel: &Arc<RwLock<CancellationToken>>,
) {
    let http = rebuild_http_client();
    let cancel_token = cancel
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
    let fresh = create_transcriber(settings, http.clone(), cancel_token);
    if let Ok(mut guard) = transcriber.write() {
        *guard = fresh;
    }
    if let Ok(mut guard) = shared_http.write() {
        *guard = http;
    }
    info!("STT engine reloaded after transient OpenAI failure");
}

async fn process_one_segment(
    job: SegmentJob,
    transcriber: Arc<RwLock<Arc<dyn TranscriptionProvider>>>,
    injector: Arc<dyn TextInjector>,
    http: reqwest::Client,
    shared_http: Arc<RwLock<reqwest::Client>>,
    cancel: Arc<RwLock<CancellationToken>>,
    pending: Arc<AtomicUsize>,
    activity_log: Arc<ActivityLog>,
    last_whisper_context: Arc<Mutex<String>>,
    llm_engine: Arc<RwLock<LlmEngine>>,
) {
    let SegmentJob {
        app,
        controller,
        mut segment,
        settings,
    } = job;

    let finish = async |pending: &Arc<AtomicUsize>, app: &AppHandle, controller: &SharedController| {
        clear_live_dictation_indicator(app, &injector).await;
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = with_controller(controller, app, |controller, handle| {
                controller.recover_after_segment(handle)
            });
            tray::menu::refresh_tray_menu(app);
        }
    };

    let child_cancel = cancel
        .read()
        .map(|guard| guard.child_token())
        .unwrap_or_else(|poisoned| poisoned.into_inner().child_token());

    if segment.is_empty() {
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Warn,
            "activity.queue.skipped_empty",
            json!({}),
        );
        finish(&pending, &app, &controller).await;
        return;
    }

    log_activity(
        &activity_log,
        Some(&app),
        ActivityLevel::Info,
        "activity.ai.transcribing",
        json!({ "ms": segment.duration_ms }),
    );

    let _ = with_controller(&controller, &app, |controller, handle| {
        emit_listening_stopped(handle);
        controller.transition_only(handle, AppState::Processing)?;
        emit_transcription_started(handle);
        controller.transition_only(handle, AppState::Transcribing)?;
        Ok(())
    });
    notify_localized(&app, &settings, "notify.transcribing", &[]);
    tray::menu::refresh_tray_menu(&app);

    if child_cancel.is_cancelled() {
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Warn,
            "activity.ai.cancelled",
            json!({}),
        );
        clear_segment(&mut segment);
        finish(&pending, &app, &controller).await;
        return;
    }

    let mut captured = segment.clone();
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
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Warn,
            "activity.segment_dropped_silence",
            json!({ "ms": captured.duration_ms }),
        );
        clear_segment(&mut segment);
        finish(&pending, &app, &controller).await;
        return;
    }
    segment = preprocessed.segment;

    let dictionary = load_dictionary(settings.transcription_dictionary_path.as_deref())
        .unwrap_or_default();
    let previous_text = last_whisper_context
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .filter(|value| !value.is_empty());
    let prompt = build_whisper_prompt(&WhisperPromptInput {
        settings: &settings,
        vocabulary: &dictionary.vocabulary,
        previous_text: previous_text.as_deref(),
    });
    let options = TranscriptionOptions {
        language: settings.language.clone(),
        prompt: prompt.clone(),
        model: settings.transcription_model.clone(),
        whisper_decoding: Some(WhisperDecodingOptions::default()),
        dictionary_path: settings.transcription_dictionary_path.clone(),
    };

    let active_transcriber = transcriber
        .read()
        .map(|guard| Arc::clone(&*guard))
        .unwrap_or_else(|poisoned| Arc::clone(&*poisoned.into_inner()));

    let mut transcription = active_transcriber
        .transcribe(segment.clone(), options.clone())
        .await;

    if matches!(transcription.as_ref(), Ok(result) if result.text.is_empty())
        && captured.duration_ms >= 400
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
            transcription = active_transcriber
                .transcribe(normalize_only.segment, options.clone())
                .await;
        }
    }

    if matches!(transcription.as_ref(), Ok(result) if result.text.is_empty())
        && captured.duration_ms >= 400
    {
        warn!(
            duration_ms = captured.duration_ms,
            "retrying transcription on raw capture audio"
        );
        let permissive = TranscriptionOptions {
            whisper_decoding: Some(WhisperDecodingOptions::permissive()),
            prompt: None,
            ..options
        };
        transcription = active_transcriber
            .transcribe(captured.clone(), permissive)
            .await;
    }

    let transcription = match transcription {
        Ok(result) => result,
        Err(error) => {
            let error_message = error.user_message(settings.ui_locale);
            if settings.uses_openai_transcription() {
                log_activity(
                    &activity_log,
                    Some(&app),
                    ActivityLevel::Warn,
                    "activity.transcription.openai_failed",
                    json!({ "error": error_message }),
                );
            }

            if error.recoverable_after_retry() {
                reload_stt_engine(&transcriber, &shared_http, &settings, &cancel);
                log_activity(
                    &activity_log,
                    Some(&app),
                    ActivityLevel::Warn,
                    "activity.transcription.engine_reloaded",
                    json!({}),
                );
                if settings.show_notifications {
                    notify_localized(
                        &app,
                        &settings,
                        "notify.openai_stt_retry_exhausted",
                        &[("error", &error_message)],
                    );
                }
                clear_segment(&mut segment);
                clear_segment(&mut captured);
                finish(&pending, &app, &controller).await;
                return;
            }

            clear_segment(&mut segment);
            clear_segment(&mut captured);
            handle_pipeline_error(
                &app,
                &controller,
                app_error_from_transcription(error, settings.ui_locale),
                &pending,
                &activity_log,
                &injector,
            )
            .await;
            return;
        }
    };

    if child_cancel.is_cancelled() {
        clear_segment(&mut segment);
        clear_segment(&mut captured);
        finish(&pending, &app, &controller).await;
        return;
    }

    let _ = emit_transcription_completed(
        &app,
        TranscriptionCompletedPayload {
            char_count: transcription.text.chars().count(),
        },
    );

    if settings.text_processing_mode.uses_ai() {
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Info,
            "activity.text.rewriting",
            json!({ "mode": settings.text_processing_mode.as_str() }),
        );
    }

    if settings.text_processing_mode.uses_ai()
        && matches!(settings.text_rewrite_provider, TextRewriteProvider::Local)
    {
        if let Err(error) = LlmEngine::ensure_loaded(&settings, &llm_engine) {
            handle_pipeline_error(
                &app,
                &controller,
                AppError::Internal(error),
                &pending,
                &activity_log,
                &injector,
            )
            .await;
            return;
        }
    }

    let llm_snapshot = llm_engine
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone());

    let processed =
        match process_transcription(
            &transcription.text,
            &settings,
            &http,
            &llm_snapshot,
            transcription.detected_language.as_deref(),
        )
        .await
        {
        Ok(processed) => processed,
        Err(error) => {
            handle_pipeline_error(
                &app,
                &controller,
                error.into(),
                &pending,
                &activity_log,
                &injector,
            )
            .await;
            return;
        }
    };

    if settings.text_processing_mode.uses_ai() {
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Info,
            "activity.text.rewrite_done",
            json!({
                "in_chars": transcription.text.chars().count(),
                "out_chars": processed.text.chars().count(),
                "fallback": processed.rewrite_fallback,
            }),
        );
    }

    if processed.rewrite_fallback {
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Warn,
            "activity.text.rewrite_fallback",
            json!({ "reason": processed.rewrite_fallback_reason.as_deref().unwrap_or("") }),
        );
        notify_localized(
            &app,
            &settings,
            "notify.rewrite_fallback",
            &[(
                "reason",
                processed.rewrite_fallback_reason.as_deref().unwrap_or(""),
            )],
        );
    }

    let mut normalized = processed.text;

    if normalized.is_empty() {
        let (capture_peak, capture_rms) = audio_peak_rms(&captured);
        let debug_wav = save_last_ptt_wav(&captured)
            .map(|path| format!(", wav: {}", path.display()))
            .unwrap_or_default();
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Warn,
            "activity.ai.empty",
            json!({}),
        );
        log_activity(
            &activity_log,
            Some(&app),
            ActivityLevel::Warn,
            "activity.ai.empty_details",
            json!({
                "peak": format!("{capture_peak:.4}"),
                "rms": format!("{capture_rms:.4}"),
                "segments": transcription.whisper_segments.unwrap_or(0).to_string(),
                "debug_wav": debug_wav,
            }),
        );
        notify_localized(&app, &settings, "notify.no_speech", &[]);
        clear_segment(&mut segment);
        clear_segment(&mut captured);
        finish(&pending, &app, &controller).await;
        return;
    }

    clear_segment(&mut segment);
    clear_segment(&mut captured);

    let char_count = normalized.chars().count();
    log_activity(
        &activity_log,
        Some(&app),
        ActivityLevel::Info,
        "activity.ai.transcribed",
        json!({ "chars": char_count }),
    );

    notify_localized(
        &app,
        &settings,
        "notify.transcribed",
        &[("preview", &preview_text(&normalized))],
    );

    let _ = with_controller(&controller, &app, |controller, handle| {
        controller.transition_only(handle, AppState::Injecting)
    });
    tray::menu::refresh_tray_menu(&app);

    notify_localized(&app, &settings, "notify.injecting", &[]);
    log_activity(
        &activity_log,
        Some(&app),
        ActivityLevel::Info,
        "activity.inject.inserting",
        json!({}),
    );

    normalized = ensure_trailing_block_separator(&normalized);

    let injection_result = if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        if ctx.live_dictation.has_injected() {
            ctx.live_dictation
                .finalize(&normalized, injector.clone(), &settings)
                .await
        } else {
            injector.insert_text(&normalized, settings.injection_mode).await
        }
    } else {
        injector.insert_text(&normalized, settings.injection_mode).await
    };

    if let Err(error) = injection_result {
        handle_pipeline_error(
            &app,
            &controller,
            error.into(),
            &pending,
            &activity_log,
            &injector,
        )
        .await;
        return;
    }

    if processed.press_enter {
        if let Err(error) = injector.send_enter().await {
            handle_pipeline_error(
                &app,
                &controller,
                error.into(),
                &pending,
                &activity_log,
                &injector,
            )
            .await;
            return;
        }
    }

    log_activity(
        &activity_log,
        Some(&app),
        ActivityLevel::Info,
        "activity.inject.done",
        json!({ "chars": char_count }),
    );
    notify_localized(
        &app,
        &settings,
        "notify.inserted",
        &[("count", &char_count.to_string())],
    );
    emit_injection_completed(&app);
    if let Ok(mut guard) = last_whisper_context.lock() {
        *guard = transcription.text.trim().to_string();
    }
    finish(&pending, &app, &controller).await;
}

async fn clear_live_dictation_indicator(app: &AppHandle, injector: &Arc<dyn TextInjector>) {
    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        return;
    };
    let settings = ctx
        .controller
        .lock()
        .ok()
        .map(|controller| controller.settings().clone());
    let Some(settings) = settings else {
        return;
    };
    ctx.live_dictation
        .clear_field_indicator(injector.clone(), &settings)
        .await;
}

fn notify_localized(
    app: &AppHandle,
    settings: &AppSettings,
    body_key: &str,
    body_args: &[(&str, &str)],
) {
    crate::notify::notify(
        app,
        &i18n::translate(settings.ui_locale, "app.title", &[]),
        &i18n::translate(settings.ui_locale, body_key, body_args),
    );
}

fn preview_text(text: &str) -> String {
    const MAX: usize = 80;
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX {
        collapsed
    } else {
        format!(
            "{}…",
            collapsed.chars().take(MAX).collect::<String>()
        )
    }
}

async fn handle_pipeline_error(
    app: &AppHandle,
    controller: &SharedController,
    error: AppError,
    pending: &Arc<AtomicUsize>,
    activity_log: &ActivityLog,
    injector: &Arc<dyn TextInjector>,
) {
    clear_live_dictation_indicator(app, injector).await;
    warn!("pipeline error: {error}");
    let locale = controller
        .lock()
        .ok()
        .map(|c| c.settings().ui_locale)
        .unwrap_or(UiLocale::En);
    log_activity(
        activity_log,
        Some(app),
        ActivityLevel::Error,
        "activity.pipeline.error",
        json!({ "error": error.to_string() }),
    );
    crate::notify::notify(
        app,
        &i18n::translate(locale, "notify.error_title", &[]),
        &error.to_string(),
    );
    let payload = error.to_payload();
    let _ = with_controller(controller, app, |controller, handle| {
        controller.report_error(handle, error);
        Ok(())
    });
    emit_error(app, payload);
    if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
        let _ = with_controller(controller, app, |controller, handle| {
            controller.recover_after_segment(handle)
        });
    }
    tray::menu::refresh_tray_menu(app);
}

fn app_error_from_transcription(
    error: crate::transcription::TranscriptionError,
    locale: UiLocale,
) -> AppError {
    let message = error.user_message(locale);
    if error.recoverable_after_retry() {
        AppError::Network(message)
    } else {
        AppError::Transcription(message)
    }
}

impl From<crate::injection::InjectionError> for AppError {
    fn from(error: crate::injection::InjectionError) -> Self {
        AppError::Injection(error.to_string())
    }
}

impl From<crate::text::TextProcessingError> for AppError {
    fn from(error: crate::text::TextProcessingError) -> Self {
        AppError::Internal(error.to_string())
    }
}
