use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::app::activity_log::{ActivityLevel, ActivityLog};
use crate::app::dictation_session::DictationSession;
use crate::app::model_idle::note_dictation_activity;
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
use crate::audio::segment_queue_store::SegmentQueueStore;
use crate::error::AppError;
use crate::injection::TextInjector;
use crate::privacy::retention::clear_segment;
use crate::i18n;
use crate::injection::InjectionError;
use crate::settings::{
    AppSettings, InjectionMode, TextProcessingMode, TextRewriteProvider, UiLocale,
};
use crate::network::HttpClient;
use crate::llm::LlmEngine;
use crate::text::normalize::{
    apply_basic_cleanup, apply_optimization_paragraphs, dedupe_near_duplicate_passages,
    dedupe_ptt_session_overlap, ensure_spaces_after_punctuation, ensure_trailing_block_separator,
};
use crate::text::dictionary::protected_terms;
use crate::text::{process_transcription, rewrite_processed_text};
use crate::transcription::prompt::{build_whisper_prompt, WhisperPromptInput};
use crate::transcription::{
    create_transcriber, TranscriptionOptions, TranscriptionProvider, WhisperDecodingOptions,
};
use crate::tray;

enum SegmentPayload {
    Memory(AudioSegment),
    Disk(PathBuf),
}

struct SegmentJob {
    app: AppHandle,
    controller: SharedController,
    payload: SegmentPayload,
    settings: AppSettings,
    held_in_memory: bool,
    session_id: u64,
}

const MAX_PENDING_SEGMENTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueResult {
    Ok { pending: usize, spilled: bool },
    DiskQueueFull,
    PendingQueueFull,
    QueueClosed,
}

pub struct PipelineRuntime {
    transcriber: Arc<RwLock<Arc<dyn TranscriptionProvider>>>,
    llm_engine: Arc<RwLock<LlmEngine>>,
    injector: Arc<dyn TextInjector>,
    cancel: Arc<RwLock<CancellationToken>>,
    pending: Arc<AtomicUsize>,
    memory_segments: Arc<AtomicUsize>,
    queue_tx: UnboundedSender<SegmentJob>,
    activity_log: Arc<ActivityLog>,
    dictation_activity_ms: Arc<AtomicU64>,
    dictation_session: Arc<DictationSession>,
}

impl PipelineRuntime {
    pub fn new(
        transcriber: Arc<dyn TranscriptionProvider>,
        injector: Arc<dyn TextInjector>,
        http: reqwest::Client,
        cancel: CancellationToken,
        activity_log: Arc<ActivityLog>,
        llm_engine: Arc<RwLock<LlmEngine>>,
        dictation_activity_ms: Arc<AtomicU64>,
        dictation_session: Arc<DictationSession>,
    ) -> Self {
        let pending = Arc::new(AtomicUsize::new(0));
        let memory_segments = Arc::new(AtomicUsize::new(0));
        let (queue_tx, mut queue_rx) = unbounded_channel();
        let transcriber = Arc::new(RwLock::new(transcriber));
        let cancel = Arc::new(RwLock::new(cancel));

        let worker_transcriber = transcriber.clone();
        let worker_llm_engine = llm_engine.clone();
        let worker_injector = injector.clone();
        let worker_http = Arc::new(RwLock::new(http));
        let worker_cancel = cancel.clone();
        let worker_pending = pending.clone();
        let worker_memory_segments = memory_segments.clone();
        let worker_log = activity_log.clone();
        let worker_dictation_activity = dictation_activity_ms.clone();
        let worker_context = Arc::new(Mutex::new(String::new()));
        let worker_dictation_session = dictation_session.clone();

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
                    worker_memory_segments.clone(),
                    worker_log.clone(),
                    worker_dictation_activity.clone(),
                    worker_context.clone(),
                    worker_llm_engine.clone(),
                    worker_dictation_session.clone(),
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
            memory_segments,
            queue_tx,
            activity_log,
            dictation_activity_ms,
            dictation_session,
        }
    }

    pub fn set_transcriber(&self, transcriber: Arc<dyn TranscriptionProvider>) {
        if let Ok(mut guard) = self.transcriber.write() {
            *guard = transcriber;
        }
    }

    /// Swap transcriber and explicitly unload the previous instance (drops native weights).
    pub async fn replace_transcriber(&self, new_transcriber: Arc<dyn TranscriptionProvider>) {
        let previous = if let Ok(mut guard) = self.transcriber.write() {
            Some(std::mem::replace(&mut *guard, new_transcriber))
        } else {
            None
        };
        if let Some(previous) = previous {
            let _ = previous.unload().await;
        }
    }

    pub async fn unload_transcriber(&self) {
        let transcriber = self
            .transcriber
            .read()
            .map(|guard| Arc::clone(&*guard))
            .ok();
        if let Some(transcriber) = transcriber {
            let _ = transcriber.unload().await;
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

    pub fn replay_spilled_segments(
        &self,
        app: AppHandle,
        controller: SharedController,
        settings: AppSettings,
    ) {
        let Ok(store) = SegmentQueueStore::for_settings(&settings) else {
            return;
        };
        for pending in store.list_pending() {
            let _ = self.enqueue_disk_replay(
                app.clone(),
                controller.clone(),
                pending.wav_path,
                settings.clone(),
            );
        }
    }

    pub fn clear_spill_queue(&self, settings: &AppSettings) {
        if let Ok(store) = SegmentQueueStore::for_settings(settings) {
            store.clear_all();
        }
        self.memory_segments.store(0, Ordering::SeqCst);
    }

    fn enqueue_disk_replay(
        &self,
        app: AppHandle,
        controller: SharedController,
        wav_path: PathBuf,
        settings: AppSettings,
    ) -> EnqueueResult {
        let duration_ms = SegmentQueueStore::load(&wav_path)
            .map(|segment| segment.duration_ms)
            .unwrap_or(0);
        self.send_job(
            app,
            controller,
            SegmentPayload::Disk(wav_path),
            settings,
            false,
            duration_ms,
            false,
        )
    }

    pub fn enqueue_segment(
        &self,
        app: AppHandle,
        controller: SharedController,
        segment: AudioSegment,
        settings: AppSettings,
    ) -> EnqueueResult {
        let duration_ms = segment.duration_ms;
        let (payload, held_in_memory, spilled) = match self.prepare_payload(segment, &settings) {
            Ok(value) => value,
            Err(()) => return EnqueueResult::DiskQueueFull,
        };
        self.send_job(
            app,
            controller,
            payload,
            settings,
            held_in_memory,
            duration_ms,
            spilled,
        )
    }

    fn prepare_payload(
        &self,
        segment: AudioSegment,
        settings: &AppSettings,
    ) -> Result<(SegmentPayload, bool, bool), ()> {
        if settings.effective_spill_enabled() {
            let cap = settings.weak_pc_ram_segment_cap.max(1) as usize;
            if self.memory_segments.load(Ordering::SeqCst) >= cap {
                let Ok(store) = SegmentQueueStore::for_settings(settings) else {
                    self.memory_segments.fetch_add(1, Ordering::SeqCst);
                    return Ok((SegmentPayload::Memory(segment), true, false));
                };
                let max_bytes = settings.weak_pc_max_disk_queue_mb as u64 * 1024 * 1024;
                let wav_bytes = (segment.samples.len() * 2).saturating_add(44) as u64;
                if store.total_bytes().saturating_add(wav_bytes) > max_bytes {
                    return Err(());
                }
                match store.spill(&segment) {
                    Ok(path) => return Ok((SegmentPayload::Disk(path), false, true)),
                    Err(error) => {
                        warn!("segment spill failed: {error}");
                        self.memory_segments.fetch_add(1, Ordering::SeqCst);
                        return Ok((SegmentPayload::Memory(segment), true, false));
                    }
                }
            }
        }

        self.memory_segments.fetch_add(1, Ordering::SeqCst);
        Ok((SegmentPayload::Memory(segment), true, false))
    }

    fn send_job(
        &self,
        app: AppHandle,
        controller: SharedController,
        payload: SegmentPayload,
        settings: AppSettings,
        held_in_memory: bool,
        duration_ms: u64,
        spilled: bool,
    ) -> EnqueueResult {
        let max_pending = MAX_PENDING_SEGMENTS.max(settings.weak_pc_ram_segment_cap as usize * 2);
        let current = self.pending.load(Ordering::SeqCst);
        if current >= max_pending {
            log_activity(
                &self.activity_log,
                &self.dictation_activity_ms,
                Some(&app),
                ActivityLevel::Warn,
                "activity.segment_dropped_queue_full",
                json!({ "ms": duration_ms, "pending": current, "max": max_pending }),
            );
            return EnqueueResult::PendingQueueFull;
        }

        self.pending.fetch_add(1, Ordering::SeqCst);
        let queued = self.pending.load(Ordering::SeqCst);
        log_activity(
            &self.activity_log,
            &self.dictation_activity_ms,
            Some(&app),
            ActivityLevel::Info,
            "activity.queue.enqueued",
            json!({ "ms": duration_ms, "pending": queued, "spilled": spilled }),
        );
        if queued > 1 {
            log_activity(
                &self.activity_log,
                &self.dictation_activity_ms,
                Some(&app),
                ActivityLevel::Info,
                "activity.queue.backlog",
                json!({ "pending": queued, "spilled": spilled }),
            );
        }

        let session_id = self.dictation_session.current_id();
        if self
            .queue_tx
            .send(SegmentJob {
                app,
                controller,
                payload,
                settings,
                held_in_memory,
                session_id,
            })
            .is_err()
        {
            self.pending.fetch_sub(1, Ordering::SeqCst);
            if held_in_memory {
                self.memory_segments.fetch_sub(1, Ordering::SeqCst);
            }
            return EnqueueResult::QueueClosed;
        }
        EnqueueResult::Ok {
            pending: queued,
            spilled,
        }
    }
}

fn log_activity(
    activity_log: &ActivityLog,
    dictation_activity_ms: &AtomicU64,
    app: Option<&AppHandle>,
    level: ActivityLevel,
    message_key: &str,
    message_args: Value,
) {
    note_dictation_activity(dictation_activity_ms, message_key);
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

pub(crate) fn should_skip_dictation_job(
    session: &DictationSession,
    session_id: u64,
    cancel: &CancellationToken,
) -> bool {
    cancel.is_cancelled() || session.is_session_aborted(session_id)
}

#[allow(clippy::too_many_arguments)]
async fn process_one_segment(
    job: SegmentJob,
    transcriber: Arc<RwLock<Arc<dyn TranscriptionProvider>>>,
    injector: Arc<dyn TextInjector>,
    http: reqwest::Client,
    shared_http: Arc<RwLock<reqwest::Client>>,
    cancel: Arc<RwLock<CancellationToken>>,
    pending: Arc<AtomicUsize>,
    memory_segments: Arc<AtomicUsize>,
    activity_log: Arc<ActivityLog>,
    dictation_activity_ms: Arc<AtomicU64>,
    last_whisper_context: Arc<Mutex<String>>,
    llm_engine: Arc<RwLock<LlmEngine>>,
    dictation_session: Arc<DictationSession>,
) {
    let SegmentJob {
        app,
        controller,
        payload,
        settings,
        held_in_memory,
        session_id,
    } = job;

    let (mut segment, spill_path) = match payload {
        SegmentPayload::Memory(segment) => (segment, None),
        SegmentPayload::Disk(path) => {
            let loaded = SegmentQueueStore::load(&path).unwrap_or_else(|error| {
                warn!(path = %path.display(), "failed to load spilled segment: {error}");
                AudioSegment::new(Vec::new(), 16_000, 1)
            });
            (loaded, Some(path))
        }
    };

    let finish = async |pending: &Arc<AtomicUsize>, app: &AppHandle, controller: &SharedController| {
        if held_in_memory {
            memory_segments.fetch_sub(1, Ordering::SeqCst);
        }
        if let Some(path) = spill_path.clone() {
            if let Ok(store) = SegmentQueueStore::for_settings(&settings) {
                store.delete_pair_for_wav(&path);
            }
        }
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = with_controller(controller, app, |controller, handle| {
                controller.recover_after_segment(handle)
            });
            try_finish_ptt_postprocess(
                app,
                controller,
                &injector,
                &llm_engine,
                &activity_log,
                &dictation_activity_ms,
                &http,
            )
            .await;
            tray::menu::refresh_tray_menu(app);
            maybe_stop_focus_watch(app);
        }
    };

    let child_cancel = cancel
        .read()
        .map(|guard| guard.child_token())
        .unwrap_or_else(|poisoned| poisoned.into_inner().child_token());

    if should_skip_dictation_job(&dictation_session, session_id, &child_cancel) {
        log_activity(
            &activity_log,
            &dictation_activity_ms,
            Some(&app),
            ActivityLevel::Warn,
            "activity.dictation.session_skipped",
            json!({ "session_id": session_id }),
        );
        clear_segment(&mut segment);
        finish(&pending, &app, &controller).await;
        return;
    }

    if segment.is_empty() {
        log_activity(
            &activity_log,
            &dictation_activity_ms,
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
        &dictation_activity_ms,
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

    if should_skip_dictation_job(&dictation_session, session_id, &child_cancel) {
        log_activity(
            &activity_log,
            &dictation_activity_ms,
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
            &dictation_activity_ms,
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

    let dictionary = crate::text::dictionary::load_dictionary_for_settings(&settings)
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
        dictionary_path: crate::settings::resolve_dictionary_file_path(&settings)
            .ok()
            .map(|path| path.display().to_string()),
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
                    &dictation_activity_ms,
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
                    &dictation_activity_ms,
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
                &dictation_activity_ms,
            )
            .await;
            return;
        }
    };

    if should_skip_dictation_job(&dictation_session, session_id, &child_cancel) {
        clear_segment(&mut segment);
        clear_segment(&mut captured);
        finish(&pending, &app, &controller).await;
        return;
    }

    emit_transcription_completed(
        &app,
        TranscriptionCompletedPayload {
            char_count: transcription.text.chars().count(),
        },
    );

    let defer_ai_postprocess = settings.ai_postprocess_mode().is_some();

    if settings.text_processing_mode.uses_ai() && !defer_ai_postprocess {
        log_activity(
            &activity_log,
            &dictation_activity_ms,
            Some(&app),
            ActivityLevel::Info,
            "activity.text.rewriting",
            json!({ "mode": settings.text_processing_mode.as_str() }),
        );
    }

    if should_skip_dictation_job(&dictation_session, session_id, &child_cancel) {
        clear_segment(&mut segment);
        clear_segment(&mut captured);
        finish(&pending, &app, &controller).await;
        return;
    }

    if settings.text_processing_mode.uses_ai()
        && !defer_ai_postprocess
        && matches!(settings.text_rewrite_provider, TextRewriteProvider::Local)
    {
        if let Err(error) = LlmEngine::ensure_loaded(&settings, &llm_engine) {
            handle_pipeline_error(
                &app,
                &controller,
                AppError::Internal(error),
                &pending,
                &activity_log,
                &dictation_activity_ms,
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
            transcription.timed_segments.as_deref(),
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
                &dictation_activity_ms,
            )
            .await;
            return;
        }
    };

    if settings.text_processing_mode.uses_ai() && !defer_ai_postprocess {
        log_activity(
            &activity_log,
            &dictation_activity_ms,
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

    if processed.rewrite_fallback && !defer_ai_postprocess {
        log_activity(
            &activity_log,
            &dictation_activity_ms,
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
            &dictation_activity_ms,
            Some(&app),
            ActivityLevel::Warn,
            "activity.ai.empty",
            json!({}),
        );
        log_activity(
            &activity_log,
            &dictation_activity_ms,
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

    log_activity(
        &activity_log,
        &dictation_activity_ms,
        Some(&app),
        ActivityLevel::Info,
        "activity.ai.transcribed",
        json!({ "chars": normalized.chars().count() }),
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
        &dictation_activity_ms,
        Some(&app),
        ActivityLevel::Info,
        "activity.inject.inserting",
        json!({}),
    );

    if should_skip_dictation_job(&dictation_session, session_id, &child_cancel) {
        finish(&pending, &app, &controller).await;
        return;
    }

    normalized = ensure_spaces_after_punctuation(&normalized);
    let continuation_outputs = dictation_session.injection_count()
        + app
            .try_state::<Arc<AppContext>>()
            .map(|ctx| ctx.focus_defer_buffer.segment_count())
            .unwrap_or(0);
    if continuation_outputs > 0 {
        normalized = crate::text::basic_cleanup::polish_continuation_injection(&normalized);
    }
    normalized = ensure_trailing_block_separator(&normalized);
    let injected_char_count = normalized.chars().count() as u32;

    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        if ctx.should_defer_injection() {
            ctx.append_deferred_injection(
                normalized.clone(),
                processed.press_enter,
                defer_ai_postprocess,
            );
            log_activity(
                &activity_log,
                &dictation_activity_ms,
                Some(&app),
                ActivityLevel::Info,
                "activity.inject.deferred",
                json!({ "chars": injected_char_count }),
            );
            if let Ok(mut guard) = last_whisper_context.lock() {
                *guard = transcription.text.trim().to_string();
            }
            finish(&pending, &app, &controller).await;
            return;
        }
    }

    let injection_result = injector
        .insert_text(&normalized, settings.injection_mode_for_host())
        .await;

    if let Err(error) = injection_result {
        handle_pipeline_error(
            &app,
            &controller,
            error.into(),
            &pending,
            &activity_log,
            &dictation_activity_ms,
        )
        .await;
        return;
    }

    dictation_session.record_injection();

    if processed.press_enter && !defer_ai_postprocess {
        if let Err(error) = injector.send_enter().await {
            handle_pipeline_error(
                &app,
                &controller,
                error.into(),
                &pending,
                &activity_log,
                &dictation_activity_ms,
            )
            .await;
            return;
        }
    }

    if defer_ai_postprocess {
        if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
            ctx.ptt_postprocess
                .record_phase1_injection(&normalized, processed.press_enter);
        }
    }

    log_activity(
        &activity_log,
        &dictation_activity_ms,
        Some(&app),
        ActivityLevel::Info,
        "activity.inject.done",
        json!({ "chars": injected_char_count }),
    );
    notify_localized(
        &app,
        &settings,
        "notify.inserted",
        &[("count", &injected_char_count.to_string())],
    );
    emit_injection_completed(&app);
    if let Ok(mut guard) = last_whisper_context.lock() {
        *guard = transcription.text.trim().to_string();
    }
    finish(&pending, &app, &controller).await;
}

fn delete_injected_session_text(
    rollback_chars: u32,
    mode: InjectionMode,
) -> Result<(), InjectionError> {
    #[cfg(windows)]
    {
        let _ = mode;
        return crate::injection::live::delete_trailing_injected_text(rollback_chars);
    }

    #[cfg(not(windows))]
    {
        let _ = mode;
        crate::injection::prepare::prepare_for_live_injection();
        if rollback_chars > 0 {
            crate::injection::keyboard_common::send_backspaces(rollback_chars)?;
        }
        Ok(())
    }
}

fn insert_injected_session_text(final_text: &str, mode: InjectionMode) -> Result<(), InjectionError> {
    if final_text.is_empty() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        let _ = mode;
        return crate::injection::live::insert_trailing_injected_text(final_text);
    }

    #[cfg(not(windows))]
    {
        let _ = mode;
        crate::injection::prepare::prepare_for_live_injection();
        crate::injection::clipboard::paste_via_clipboard(final_text)
    }
}

fn replace_injected_session_text(
    rollback_chars: u32,
    final_text: &str,
    mode: InjectionMode,
) -> Result<(), InjectionError> {
    delete_injected_session_text(rollback_chars, mode)?;
    insert_injected_session_text(final_text, mode)
}

pub(crate) fn schedule_ptt_postprocess_finish(app: AppHandle, ctx: Arc<AppContext>) {
    let controller = ctx.controller.clone();
    let injector = ctx.runtime.injector();
    let llm_engine = ctx.llm_engine.clone();
    let activity_log = ctx.activity_log.clone();
    let dictation_activity_ms = ctx.llm_dictation_activity_ms.clone();
    let http = ctx.http.clone();
    tauri::async_runtime::spawn(async move {
        try_finish_ptt_postprocess(
            &app,
            &controller,
            &injector,
            &llm_engine,
            &activity_log,
            &dictation_activity_ms,
            &http,
        )
        .await;
    });
}

async fn try_finish_ptt_postprocess(
    app: &AppHandle,
    controller: &SharedController,
    injector: &Arc<dyn TextInjector>,
    llm_engine: &Arc<RwLock<LlmEngine>>,
    activity_log: &Arc<ActivityLog>,
    dictation_activity_ms: &AtomicU64,
    http: &reqwest::Client,
) {
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        if ctx.runtime.cancel_token().is_cancelled() {
            ctx.ptt_postprocess.reset();
            return;
        }
    }

    let ptt_active = controller
        .lock()
        .ok()
        .is_some_and(|c| c.is_push_to_talk_active());
    if ptt_active {
        return;
    }

    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        return;
    };

    if ctx.runtime.pending_count() > 0 {
        return;
    }

    if ctx.focus_defer_buffer.has_pending() {
        return;
    }

    if !ctx.ptt_postprocess.try_begin_finish() {
        return;
    }

    let _finish_guard = PttFinishGuard(&ctx.ptt_postprocess);

    let settings = match controller.lock() {
        Ok(c) => c.settings().clone(),
        Err(_) => return,
    };

    let Some(ai_mode) = settings.ai_postprocess_mode() else {
        ctx.ptt_postprocess.reset();
        return;
    };

    if ctx.runtime.pending_count() > 0 {
        return;
    }

    let Some((injected_text, press_enter)) = ctx.ptt_postprocess.take_finish_snapshot() else {
        return;
    };

    let rollback_chars = injected_text.chars().count() as u32;
    if injected_text.is_empty() || rollback_chars == 0 {
        return;
    }

    let rewrite_input = apply_basic_cleanup(&dedupe_ptt_session_overlap(injected_text.trim()));
    if rewrite_input.is_empty() {
        return;
    }

    log_activity(
        activity_log,
        dictation_activity_ms,
        Some(app),
        ActivityLevel::Info,
        "activity.text.rewriting",
        json!({ "mode": ai_mode.as_str(), "session": true }),
    );

    if matches!(settings.text_rewrite_provider, TextRewriteProvider::Local) {
        if let Err(error) = LlmEngine::ensure_loaded(&settings, llm_engine) {
            warn!("PTT AI postprocess: LLM load failed: {error}");
            return;
        }
    }

    let llm_snapshot = llm_engine
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone());

    let dictionary = crate::text::dictionary::load_dictionary_for_settings(&settings)
        .unwrap_or_default();
    let terms = protected_terms(&dictionary);

    let _ = with_controller(controller, app, |controller, handle| {
        controller.transition_only(handle, AppState::Processing)
    });

    tray::blink::start_purple_blink(app);

    let injection_mode = settings.injection_mode_for_host();
    let rollback_for_delete = rollback_chars;
    let rewrite_fut = rewrite_processed_text(
        &rewrite_input,
        ai_mode,
        &settings,
        settings.ai_rewrite_skill.as_deref(),
        http,
        &llm_snapshot,
        &terms,
        &dictionary,
        press_enter,
    );
    let delete_fut = tokio::task::spawn_blocking(move || {
        delete_injected_session_text(rollback_for_delete, injection_mode)
    });

    let (rewrite_result, delete_result) = tokio::join!(rewrite_fut, delete_fut);

    tray::blink::stop_purple_blink(app);

    let delete_ok = matches!(&delete_result, Ok(Ok(())));
    if let Err(error) = delete_result {
        warn!("PTT AI postprocess delete task failed: {error}");
    } else if let Ok(Err(error)) = delete_result {
        warn!("PTT AI postprocess delete failed: {error}");
    }

    let processed = match rewrite_result {
        Ok(processed) => processed,
        Err(error) => {
            warn!("PTT AI postprocess failed, keeping phase-1 text: {error}");
            log_activity(
                activity_log,
                dictation_activity_ms,
                Some(app),
                ActivityLevel::Warn,
                "activity.text.rewrite_fallback",
                json!({ "reason": error.to_string(), "session": true }),
            );
            restore_phase1_injected_text(delete_ok, injection_mode, &rewrite_input).await;
            let _ = with_controller(controller, app, |controller, handle| {
                controller.recover_to_ready(handle)
            });
            return;
        }
    };

    log_activity(
        activity_log,
        dictation_activity_ms,
        Some(app),
        ActivityLevel::Info,
        "activity.text.rewrite_done",
        json!({
            "in_chars": rewrite_input.chars().count(),
            "out_chars": processed.text.chars().count(),
            "fallback": processed.rewrite_fallback,
            "session": true,
        }),
    );

    if processed.rewrite_fallback {
        log_activity(
            activity_log,
            dictation_activity_ms,
            Some(app),
            ActivityLevel::Warn,
            "activity.text.rewrite_fallback",
            json!({
                "reason": processed.rewrite_fallback_reason.as_deref().unwrap_or(""),
                "session": true,
            }),
        );
        notify_localized(
            app,
            &settings,
            "notify.rewrite_fallback",
            &[(
                "reason",
                processed.rewrite_fallback_reason.as_deref().unwrap_or(""),
            )],
        );
    }

    let mut cleaned = ensure_spaces_after_punctuation(&dedupe_near_duplicate_passages(
        &processed.text,
    ));
    if ai_mode == TextProcessingMode::Optimization {
        cleaned = apply_optimization_paragraphs(&cleaned);
    }
    let final_text = ensure_trailing_block_separator(&cleaned);
    if final_text.is_empty() {
        restore_phase1_injected_text(delete_ok, injection_mode, &rewrite_input).await;
        let _ = with_controller(controller, app, |controller, handle| {
            controller.recover_to_ready(handle)
        });
        return;
    }

    if ctx.runtime.pending_count() > 0 {
        warn!("PTT AI postprocess: replace skipped, segments still pending");
        restore_phase1_injected_text(delete_ok, injection_mode, &rewrite_input).await;
        return;
    }

    let _ = with_controller(controller, app, |controller, handle| {
        controller.transition_only(handle, AppState::Injecting)
    });

    let replace_result = tokio::task::spawn_blocking({
        let final_text = final_text.clone();
        move || {
            if delete_ok {
                insert_injected_session_text(&final_text, injection_mode)
            } else {
                replace_injected_session_text(rollback_chars, &final_text, injection_mode)
            }
        }
    })
    .await;

    match replace_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!("PTT AI postprocess replace failed: {error}");
            restore_phase1_injected_text(delete_ok, injection_mode, &rewrite_input).await;
            return;
        }
        Err(error) => {
            warn!("PTT AI postprocess replace task failed: {error}");
            restore_phase1_injected_text(delete_ok, injection_mode, &rewrite_input).await;
            return;
        }
    }

    if processed.press_enter {
        if let Err(error) = injector.send_enter().await {
            warn!("PTT AI postprocess enter failed: {error}");
        }
    }

    notify_localized(
        app,
        &settings,
        "notify.inserted",
        &[("count", &final_text.chars().count().to_string())],
    );
    emit_injection_completed(app);

    let _ = with_controller(controller, app, |controller, handle| {
        controller.recover_to_ready(handle)
    });
}

struct PttFinishGuard<'a>(&'a crate::app::ptt_postprocess::PttPostprocessSession);

impl Drop for PttFinishGuard<'_> {
    fn drop(&mut self) {
        self.0.finish_finish();
    }
}

async fn restore_phase1_injected_text(
    delete_succeeded: bool,
    injection_mode: InjectionMode,
    text: &str,
) {
    if !delete_succeeded || text.is_empty() {
        return;
    }
    let text = text.to_string();
    match tokio::task::spawn_blocking(move || insert_injected_session_text(&text, injection_mode))
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!("PTT AI postprocess: failed to restore phase-1 text: {error}");
        }
        Err(error) => {
            warn!("PTT AI postprocess: restore phase-1 task failed: {error}");
        }
    }
}

fn notify_localized(
    app: &AppHandle,
    settings: &AppSettings,
    body_key: &str,
    body_args: &[(&str, &str)],
) {
    if settings.suppress_ptt_toasts() {
        return;
    }
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
    dictation_activity_ms: &AtomicU64,
) {
    warn!("pipeline error: {error}");
    let locale = controller
        .lock()
        .ok()
        .map(|c| c.settings().ui_locale)
        .unwrap_or(UiLocale::En);
    log_activity(
        activity_log,
        dictation_activity_ms,
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

pub async fn flush_focus_defer_buffer(app: &AppHandle, ctx: &Arc<AppContext>) {
    #[cfg(windows)]
    if !crate::injection::focus_target::focus_target_matches() {
        return;
    }
    let Some((text, press_enter)) = ctx.focus_defer_buffer.take_for_flush() else {
        return;
    };
    if text.is_empty() {
        ctx.maybe_stop_focus_watch();
        return;
    }

    let controller = ctx.controller.clone();
    let activity_log = ctx.activity_log.clone();
    let dictation_activity_ms = ctx.llm_dictation_activity_ms.clone();
    let llm_engine = ctx.llm_engine.clone();
    let http = ctx.http.clone();

    let settings = match controller.lock() {
        Ok(c) => c.settings().clone(),
        Err(_) => {
            ctx.focus_defer_buffer.push_prepared(text, press_enter);
            return;
        }
    };
    let injector = ctx.runtime.injector();
    let char_count = text.chars().count() as u32;

    let _ = with_controller(&controller, app, |controller, handle| {
        controller.transition_for_injection(handle)
    });
    tray::menu::refresh_tray_menu(app);

    let injection_result = injector
        .insert_text(&text, settings.injection_mode_for_host())
        .await;

    if let Err(error) = injection_result {
        warn!("focus defer flush failed: {error}");
        ctx.focus_defer_buffer.push_prepared(text, press_enter);
        log_activity(
            &activity_log,
            &dictation_activity_ms,
            Some(app),
            ActivityLevel::Error,
            "activity.pipeline.error",
            json!({ "error": error.to_string() }),
        );
        let _ = with_controller(&controller, app, |controller, handle| {
            controller.recover_to_ready(handle)
        });
        return;
    }

    ctx.dictation_session.record_injection();

    if press_enter {
        if let Err(error) = injector.send_enter().await {
            warn!("focus defer flush enter failed: {error}");
        }
    }

    log_activity(
        &activity_log,
        &dictation_activity_ms,
        Some(app),
        ActivityLevel::Info,
        "activity.inject.defer_flushed",
        json!({ "chars": char_count }),
    );
    notify_localized(
        app,
        &settings,
        "notify.inserted",
        &[("count", &char_count.to_string())],
    );
    emit_injection_completed(app);

    try_finish_ptt_postprocess(
        app,
        &controller,
        &injector,
        &llm_engine,
        &activity_log,
        &dictation_activity_ms,
        &http,
    )
    .await;

    if ctx.runtime.pending_count() == 0 {
        let _ = with_controller(&controller, app, |controller, handle| {
            controller.recover_after_segment(handle)
        });
    }
    ctx.maybe_stop_focus_watch();
}

fn maybe_stop_focus_watch(app: &AppHandle) {
    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        return;
    };
    ctx.maybe_stop_focus_watch();
}

#[cfg(test)]
mod dictation_skip_tests {
    use super::*;

    #[test]
    fn skip_when_session_aborted() {
        let session = DictationSession::new();
        let id = session.begin_session();
        session.abort_current();
        let cancel = CancellationToken::new();
        assert!(should_skip_dictation_job(&session, id, &cancel));
    }

    #[test]
    fn skip_when_cancelled() {
        let session = DictationSession::new();
        let id = session.begin_session();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(should_skip_dictation_job(&session, id, &cancel));
    }
}
