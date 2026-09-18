use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use tauri::AppHandle;
use tracing::{debug, warn};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::app::events::{emit_transcription_partial, emit_transcription_partial_clear};
use crate::audio::preprocess::{preprocess_segment, PreprocessOptions};
use crate::audio::preview::trim_for_preview;
use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
use crate::text::pipeline::process_transcription_immediate_sync;
use crate::transcription::{TranscriptionOptions, WhisperDecodingOptions};

struct PreviewJob {
    app: AppHandle,
    ctx: Arc<AppContext>,
    segment: AudioSegment,
    session_id: u64,
}

struct StreamingPreviewInner {
    active: AtomicBool,
    /// Bumped only on PTT start/stop to invalidate cross-session preview jobs.
    session_id: AtomicU64,
    pending: Mutex<Option<PreviewJob>>,
    job_tx: Mutex<Option<Sender<PreviewJob>>>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

pub struct StreamingPreview {
    inner: Arc<StreamingPreviewInner>,
}

impl Default for StreamingPreview {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingPreview {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StreamingPreviewInner {
                active: AtomicBool::new(false),
                session_id: AtomicU64::new(0),
                pending: Mutex::new(None),
                job_tx: Mutex::new(None),
                worker: Mutex::new(None),
            }),
        }
    }

    pub fn start(&self) {
        self.inner.active.store(true, Ordering::SeqCst);
        self.inner.session_id.fetch_add(1, Ordering::SeqCst);
        ensure_worker(&self.inner);
    }

    pub fn stop_and_clear(&self, app: &AppHandle) {
        self.inner.active.store(false, Ordering::SeqCst);
        self.inner.session_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut pending) = self.inner.pending.lock() {
            pending.take();
        }
        emit_transcription_partial_clear(app);
    }

    pub fn handle_snapshot(&self, app: AppHandle, ctx: Arc<AppContext>, segment: AudioSegment) {
        if !self.inner.active.load(Ordering::SeqCst) && !ctx.live_dictation.is_active() {
            return;
        }

        let live_mode = ctx.live_dictation.is_active();
        if live_mode {
            return;
        }

        let segment = {
            let Some(segment) = trim_for_preview(&segment) else {
                return;
            };
            segment
        };

        ensure_worker(&self.inner);

        ctx.record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.live.queued",
            serde_json::json!({ "ms": segment.duration_ms }),
        );

        let session_id = self.inner.session_id.load(Ordering::SeqCst);
        enqueue_preview(
            Arc::clone(&self.inner),
            PreviewJob {
                app,
                ctx,
                segment,
                session_id,
            },
        );
    }
}

fn preview_job_stale(inner: &StreamingPreviewInner, session_id: u64) -> bool {
    inner.session_id.load(Ordering::SeqCst) != session_id
}

fn ensure_worker(inner: &Arc<StreamingPreviewInner>) {
    let mut worker = match inner.worker.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    if worker.is_some() {
        return;
    }

    let (job_tx, job_rx) = bounded(1);
    if let Ok(mut sender) = inner.job_tx.lock() {
        *sender = Some(job_tx);
    }

    let worker_inner = Arc::clone(inner);
    *worker = Some(
        thread::Builder::new()
            .name("preview-worker".into())
            .spawn(move || preview_worker_loop(worker_inner, job_rx))
            .expect("preview worker thread"),
    );
}

fn sender(inner: &Arc<StreamingPreviewInner>) -> Option<Sender<PreviewJob>> {
    inner.job_tx.lock().ok().and_then(|guard| guard.clone())
}

fn enqueue_preview(inner: Arc<StreamingPreviewInner>, job: PreviewJob) {
    let Some(job_tx) = sender(&inner) else {
        if let Ok(mut pending) = inner.pending.lock() {
            *pending = Some(job);
        }
        return;
    };

    match job_tx.try_send(job) {
        Ok(()) => {}
        Err(TrySendError::Full(job)) => {
            if let Ok(mut pending) = inner.pending.lock() {
                *pending = Some(job);
            }
            debug!("preview worker busy; coalesced latest snapshot");
        }
        Err(TrySendError::Disconnected(_)) => {
            warn!("preview worker channel disconnected; restarting worker");
            if let Ok(mut worker) = inner.worker.lock() {
                worker.take();
            }
            if let Ok(mut tx) = inner.job_tx.lock() {
                tx.take();
            }
            ensure_worker(&inner);
        }
    }
}

fn preview_worker_loop(inner: Arc<StreamingPreviewInner>, job_rx: Receiver<PreviewJob>) {
    while let Ok(mut job) = job_rx.recv() {
        while let Ok(next) = job_rx.try_recv() {
            job = next;
        }

        if !inner.active.load(Ordering::SeqCst) && !job.ctx.live_dictation.is_active() {
            continue;
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_preview_job(job, Arc::clone(&inner));
        }));

        if result.is_err() {
            warn!("preview worker panicked while processing a snapshot");
        }
    }
}

struct PendingDrain {
    inner: Arc<StreamingPreviewInner>,
    ctx: Arc<AppContext>,
}

impl Drop for PendingDrain {
    fn drop(&mut self) {
        if !self.inner.active.load(Ordering::SeqCst) && !self.ctx.live_dictation.is_active() {
            return;
        }

        let next_job = self
            .inner
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.take());
        if let Some(next_job) = next_job {
            enqueue_preview(Arc::clone(&self.inner), next_job);
        }
    }
}

fn run_preview_job(job: PreviewJob, inner: Arc<StreamingPreviewInner>) {
    let PreviewJob {
        app,
        ctx,
        segment,
        session_id,
    } = job;
    let _pending_drain = PendingDrain {
        inner: Arc::clone(&inner),
        ctx: Arc::clone(&ctx),
    };

    if preview_job_stale(&inner, session_id) {
        return;
    }

    let settings = match ctx.controller.try_lock() {
        Ok(controller) => controller.settings().clone(),
        Err(_) => {
            tracing::debug!("live preview skipped: controller lock busy");
            return;
        }
    };

    if settings.transcription_provider != "local" {
        ctx.record_activity(
            Some(&app),
            ActivityLevel::Warn,
            "activity.live.cloud_unsupported",
            serde_json::json!({}),
        );
        return;
    }

    let duration_ms = segment.duration_ms;
    let Some(segment) = prepare_preview_segment(segment, &settings) else {
        ctx.record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.live.empty",
            serde_json::json!({ "ms": duration_ms, "reason": "silence" }),
        );
        return;
    };
    if segment.is_empty() {
        ctx.record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.live.empty",
            serde_json::json!({ "ms": duration_ms, "reason": "no_samples" }),
        );
        return;
    }

    let runtime = ctx.runtime.clone();
    let options = preview_options(&settings);
    let duration_ms = segment.duration_ms;
    match runtime.transcriber().transcribe_preview_sync(segment, options) {
        Ok(Some(text)) => {
            if preview_job_stale(&inner, session_id) {
                return;
            }

            let preview_text = if settings.ai_postprocess_mode().is_some() {
                process_transcription_immediate_sync(&text, None, &settings, None)
                    .map(|processed| processed.text)
                    .unwrap_or(text)
            } else {
                text
            };

            emit_transcription_partial(&app, &preview_text);
            ctx.record_activity(
                Some(&app),
                ActivityLevel::Info,
                "activity.live.preview",
                serde_json::json!({
                    "ms": duration_ms,
                    "chars": preview_text.chars().count(),
                    "text": preview_text,
                }),
            );
        }
        Ok(None) => {
            ctx.record_activity(
                Some(&app),
                ActivityLevel::Info,
                "activity.live.empty",
                serde_json::json!({ "ms": duration_ms }),
            );
        }
        Err(error) => {
            warn!("streaming preview failed: {error}");
            ctx.record_activity(
                Some(&app),
                ActivityLevel::Warn,
                "activity.live.preview_failed",
                serde_json::json!({ "error": error.to_string() }),
            );
        }
    }

}

fn prepare_preview_segment(segment: AudioSegment, settings: &AppSettings) -> Option<AudioSegment> {
    let preprocessed = preprocess_segment(
        segment.clone(),
        PreprocessOptions {
            enabled: settings.audio_preprocess_enabled,
            noise_reduction_enabled: settings.audio_preprocess_enabled
                && settings.audio_noise_reduction_enabled,
            normalize_only: false,
        },
    );
    if !preprocessed.skipped_as_silence {
        return Some(preprocessed.segment);
    }

    let normalize_only = preprocess_segment(
        segment,
        PreprocessOptions {
            enabled: true,
            noise_reduction_enabled: false,
            normalize_only: true,
        },
    );
    if normalize_only.skipped_as_silence {
        None
    } else {
        Some(normalize_only.segment)
    }
}

fn preview_options(settings: &AppSettings) -> TranscriptionOptions {
    TranscriptionOptions {
        language: settings.language.clone(),
        // Live preview skips initial_prompt entirely: faster decode and no prompt-echo stalls.
        prompt: None,
        model: settings.transcription_model.clone(),
        whisper_decoding: Some(WhisperDecodingOptions::permissive()),
        dictionary_path: settings.transcription_dictionary_path.clone(),
    }
}
