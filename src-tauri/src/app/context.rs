use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex, RwLock,
};

use crossbeam_channel::bounded;

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use serde_json::Value;
use crate::app::activity_log::{ActivityLevel, ActivityLog};
use crate::app::controller::{AppController, SharedController};
use crate::app::events::emit_activity_log;
use crate::app::runtime::PipelineRuntime;
use crate::audio::monitor::MicMonitor;
use crate::audio::pipeline::AudioPipeline;
use crate::injection::TextInjector;
use crate::app::controller::SettingsUpdatePlan;
use crate::app::memory::{collect_memory_snapshot, needs_whisper_prewarm, MemorySnapshot};
use crate::llm::LlmEngine;
use crate::llm::model_store::needs_local_llm;
use crate::settings::{has_api_key, AppSettings};
use crate::transcription::{
    create_transcriber, streaming::StreamingPreview, LiveDictationSession, TranscriptionProvider,
};

pub struct AppContext {
    pub controller: SharedController,
    pub audio: Mutex<AudioPipeline>,
    pub runtime: Arc<PipelineRuntime>,
    root_cancel: CancellationToken,
    transcriber_cancel: Mutex<CancellationToken>,
    pub http: reqwest::Client,
    audio_callbacks_enabled: Arc<AtomicBool>,
    pub activity_log: Arc<ActivityLog>,
    /// Serializes PTT press/release so release cannot run before press finishes.
    pub ptt_lock: Mutex<()>,
    pub mic_level: Arc<AtomicU32>,
    pub mic_monitor: MicMonitor,
    pub llm_engine: Arc<RwLock<LlmEngine>>,
    pub streaming_preview: StreamingPreview,
    pub live_dictation: LiveDictationSession,
}

impl AppContext {
    pub fn new(
        controller: AppController,
        transcriber: Arc<dyn TranscriptionProvider>,
        injector: Arc<dyn TextInjector>,
        http: reqwest::Client,
        transcriber_cancel: CancellationToken,
        settings: &AppSettings,
    ) -> Self {
        let root_cancel = CancellationToken::new();
        let activity_log = Arc::new(ActivityLog::new());
        let llm_engine = Arc::new(RwLock::new(LlmEngine::unloaded(None)));
        let runtime = Arc::new(PipelineRuntime::new(
            transcriber,
            injector,
            http.clone(),
            root_cancel.clone(),
            activity_log.clone(),
            llm_engine
                .read()
                .map(|guard| guard.clone())
                .unwrap_or_else(|poisoned| poisoned.into_inner().clone()),
        ));
        let mic_monitor = MicMonitor::new();
        let mut audio = AudioPipeline::new(settings.vad_config());
        audio.set_mic_monitor(mic_monitor.clone());
        let mic_level = audio.mic_level_atomic();

        Self {
            controller: Arc::new(Mutex::new(controller)),
            audio: Mutex::new(audio),
            mic_level,
            mic_monitor,
            runtime,
            root_cancel,
            transcriber_cancel: Mutex::new(transcriber_cancel),
            http,
            audio_callbacks_enabled: Arc::new(AtomicBool::new(true)),
            activity_log,
            ptt_lock: Mutex::new(()),
            llm_engine: llm_engine.clone(),
            streaming_preview: StreamingPreview::new(),
            live_dictation: LiveDictationSession::new(),
        }
    }

    pub async fn unload_transcriber(&self) {
        self.runtime.unload_transcriber().await;
    }

    pub fn start_live_dictation(&self) {
        let field_indicator = self
            .controller
            .try_lock()
            .ok()
            .map(|controller| {
                controller.settings().live_dictation_field_indicator
                    && controller.settings().ptt_hold
            })
            .unwrap_or(false);
        self.live_dictation.start(field_indicator);
    }

    fn transcriber_cancel_token(&self) -> CancellationToken {
        self.transcriber_cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Cancel in-flight STT work and issue a fresh token for the next transcriber instance.
    fn rotate_transcriber_cancel(&self) -> CancellationToken {
        let mut guard = self
            .transcriber_cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.cancel();
        *guard = CancellationToken::new();
        guard.clone()
    }

    pub fn reload_transcriber(&self, settings: &AppSettings) {
        let transcriber = create_transcriber(
            settings,
            self.http.clone(),
            self.transcriber_cancel_token(),
        );
        self.runtime.set_transcriber(transcriber);
    }

    pub fn sync_llm_engine(&self, settings: &AppSettings) {
        if needs_local_llm(settings) {
            let previous = self.llm_engine.write().ok().map(|mut guard| {
                std::mem::replace(
                    &mut *guard,
                    LlmEngine::unloaded(Some("reloading local LLM".to_string())),
                )
            });
            if let Some(previous) = previous {
                if previous.is_ready() {
                    previous.unload();
                }
            }

            let engine = LlmEngine::from_settings(settings);
            if let Ok(mut guard) = self.llm_engine.write() {
                *guard = engine.clone();
            }
            self.runtime.set_llm_engine(engine);
            return;
        }

        let previous = self.llm_engine.write().ok().map(|mut guard| {
            std::mem::replace(
                &mut *guard,
                LlmEngine::unloaded(Some(
                    "local LLM is not required for current settings".to_string(),
                )),
            )
        });
        if let Some(previous) = previous {
            if previous.is_ready() {
                previous.unload();
            }
        }
        let unloaded = self
            .llm_engine
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        self.runtime.set_llm_engine(unloaded);
    }

    pub fn ensure_local_llm(&self, settings: &AppSettings) -> Result<(), String> {
        LlmEngine::ensure_loaded(settings, &self.llm_engine)?;
        let engine = self
            .llm_engine
            .read()
            .map_err(|_| "local LLM engine lock poisoned".to_string())?
            .clone();
        self.runtime.set_llm_engine(engine);
        Ok(())
    }

    pub async fn apply_memory_policy(
        &self,
        app: Option<&AppHandle>,
        settings: &AppSettings,
        plan: &SettingsUpdatePlan,
    ) {
        let llm_was_ready = self.llm_engine.read().map(|e| e.is_ready()).unwrap_or(false);
        let whisper_was_loaded = self.runtime.is_whisper_model_loaded();

        if plan.reload_transcriber {
            let _ = self.rotate_transcriber_cancel();
            self.unload_transcriber().await;
            self.reload_transcriber(settings);
        } else if !needs_whisper_prewarm(settings) && whisper_was_loaded {
            let _ = self.rotate_transcriber_cancel();
            self.unload_transcriber().await;
        }

        if plan.reload_llm_engine || !needs_local_llm(settings) {
            self.sync_llm_engine(settings);
        }

        if llm_was_ready && !needs_local_llm(settings) {
            self.record_activity(
                app,
                ActivityLevel::Info,
                "activity.memory.llm_unloaded",
                Value::Object(Default::default()),
            );
        }
        if whisper_was_loaded && !needs_whisper_prewarm(settings) {
            self.record_activity(
                app,
                ActivityLevel::Info,
                "activity.memory.whisper_unloaded",
                Value::Object(Default::default()),
            );
        }

        let _snapshot = app.map(|handle| collect_memory_snapshot(handle, self));
        let _ = _snapshot.unwrap_or(MemorySnapshot {
            whisper_loaded: self.runtime.is_whisper_model_loaded(),
            llm_loaded: self.llm_engine.read().map(|e| e.is_ready()).unwrap_or(false),
            settings_webview_alive: false,
            about_webview_alive: false,
        });
    }

    pub fn audio_callbacks_enabled(&self) -> bool {
        self.audio_callbacks_enabled.load(Ordering::SeqCst)
    }

    pub fn set_audio_callbacks_enabled(&self, enabled: bool) {
        self.audio_callbacks_enabled
            .store(enabled, Ordering::SeqCst);
    }

    pub fn record_activity(
        &self,
        app: Option<&AppHandle>,
        level: ActivityLevel,
        message_key: &str,
        message_args: Value,
    ) {
        self.activity_log.push(level, message_key, message_args);

        if let Some(app) = app {
            emit_activity_log(app, &self.activity_log.snapshot());
        }
    }

    pub fn wire_audio_callbacks(self: &Arc<Self>, app: &tauri::AppHandle) {
        let ctx = Arc::clone(self);
        let app_handle = app.clone();
        let callbacks_enabled = Arc::clone(&self.audio_callbacks_enabled);

        let on_speech_started = Arc::new(move || {
            if !callbacks_enabled.load(Ordering::SeqCst) {
                ctx.record_activity(
                    Some(&app_handle),
                    ActivityLevel::Warn,
                    "activity.speech_ignored_lock",
                    Value::Object(Default::default()),
                );
                return;
            }

            let Ok(mut controller) = ctx.controller.try_lock() else {
                ctx.record_activity(
                    Some(&app_handle),
                    ActivityLevel::Warn,
                    "activity.speech_controller_busy",
                    Value::Object(Default::default()),
                );
                return;
            };

            ctx.record_activity(
                Some(&app_handle),
                ActivityLevel::Info,
                "activity.vad.speech_started",
                Value::Object(Default::default()),
            );

            let stream_live = !controller.status().ptt_hold;
            let _ = controller.on_speech_started(&app_handle);
            drop(controller);

            if stream_live {
                crate::injection::focus_target::capture_injection_target();
                ctx.streaming_preview.start();
                ctx.start_live_dictation();
            }
        });

        let ctx = Arc::clone(self);
        let app_handle = app.clone();
        let callbacks_enabled = Arc::clone(&self.audio_callbacks_enabled);

        let on_speech_ended = Arc::new(move |segment: crate::audio::segment::AudioSegment| {
            if !callbacks_enabled.load(Ordering::SeqCst) {
                ctx.record_activity(
                    Some(&app_handle),
                    ActivityLevel::Warn,
                    "activity.segment_dropped_lock",
                    serde_json::json!({ "ms": segment.duration_ms }),
                );
                return;
            }

            if segment.is_empty() {
                ctx.record_activity(
                    Some(&app_handle),
                    ActivityLevel::Warn,
                    "activity.segment_dropped_empty",
                    Value::Object(Default::default()),
                );
                return;
            }

            let settings = ctx
                .controller
                .try_lock()
                .ok()
                .map(|controller| controller.settings().clone());

            let Some(settings) = settings else {
                ctx.record_activity(
                    Some(&app_handle),
                    ActivityLevel::Warn,
                    "activity.segment_dropped_busy",
                    Value::Object(Default::default()),
                );
                return;
            };

            if settings.requires_api_key_for_transcription() && !has_api_key() {
                ctx.record_activity(
                    Some(&app_handle),
                    ActivityLevel::Error,
                    "activity.segment_no_api_key",
                    serde_json::json!({ "ms": segment.duration_ms }),
                );
                return;
            }

            ctx.record_activity(
                Some(&app_handle),
                ActivityLevel::Info,
                "activity.vad.segment_ended",
                serde_json::json!({ "ms": segment.duration_ms }),
            );

            ctx.streaming_preview.stop_and_clear(&app_handle);

            ctx.runtime.process_segment(
                app_handle.clone(),
                ctx.controller.clone(),
                segment,
                settings,
            );
        });

        let (preview_tx, preview_rx) =
            bounded::<crate::audio::segment::AudioSegment>(4);
        let ctx_preview = Arc::clone(self);
        let app_preview = app.clone();
        std::thread::Builder::new()
            .name("live-preview".into())
            .spawn(move || {
                while let Ok(segment) = preview_rx.recv() {
                    ctx_preview.record_activity(
                        Some(&app_preview),
                        ActivityLevel::Info,
                        "activity.live.snapshot",
                        serde_json::json!({ "ms": segment.duration_ms }),
                    );

                    ctx_preview.streaming_preview.handle_snapshot(
                        app_preview.clone(),
                        Arc::clone(&ctx_preview),
                        segment,
                    );
                }
            })
            .expect("live preview receiver thread");

        match self.audio.lock() {
            Ok(mut audio) => {
                audio.set_callbacks(on_speech_started, on_speech_ended);
                audio.set_preview_sender(preview_tx);
            }
            Err(_) => {
                self.record_activity(
                    Some(app),
                    ActivityLevel::Error,
                    "activity.audio.callback_wire_failed",
                    Value::Object(Default::default()),
                );
            }
        }

        self.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.ready",
            Value::Object(Default::default()),
        );
    }

    pub fn cancel_pending(&self) {
        self.runtime.cancel_pending();
        self.root_cancel.cancel();
    }
}
