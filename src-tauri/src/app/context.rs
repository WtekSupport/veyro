use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering},
    Arc, Mutex, RwLock,
};

use crossbeam_channel::bounded;

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use serde_json::Value;
use crate::app::activity_log::{ActivityLevel, ActivityLog};
use crate::app::controller::{AppController, SharedController};
use crate::app::ptt_postprocess::PttPostprocessSession;
use crate::app::events::emit_activity_log;
use crate::app::runtime::PipelineRuntime;
use crate::audio::monitor::MicMonitor;
use crate::audio::pipeline::AudioPipeline;
use crate::injection::TextInjector;
use crate::app::controller::SettingsUpdatePlan;
use crate::app::model_idle::{dictation_activity_now_ms, note_dictation_activity};
use crate::app::memory::{collect_memory_snapshot, needs_local_stt, MemorySnapshot};
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
    pub llm_dictation_activity_ms: Arc<AtomicU64>,
    pub streaming_preview: StreamingPreview,
    pub live_dictation: LiveDictationSession,
    pub ptt_postprocess: PttPostprocessSession,
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
        let llm_dictation_activity_ms =
            Arc::new(AtomicU64::new(dictation_activity_now_ms()));
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
            Arc::clone(&llm_dictation_activity_ms),
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
            llm_dictation_activity_ms,
            streaming_preview: StreamingPreview::new(),
            live_dictation: LiveDictationSession::new(),
            ptt_postprocess: PttPostprocessSession::new(),
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
                let settings = controller.settings();
                settings.push_to_talk && settings.ptt_hold
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
        Self::sync_llm_engine_owned(settings, &self.llm_engine, &self.runtime, false);
    }

    fn sync_llm_engine_owned(
        settings: &AppSettings,
        llm_engine: &Arc<RwLock<LlmEngine>>,
        runtime: &Arc<PipelineRuntime>,
        defer_load: bool,
    ) {
        if needs_local_llm(settings) {
            let previous = llm_engine.write().ok().map(|mut guard| {
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

            let engine = if defer_load {
                LlmEngine::unloaded(Some(
                    "local LLM unloaded; will load on first rewrite".to_string(),
                ))
            } else {
                LlmEngine::from_settings(settings)
            };
            if let Ok(mut guard) = llm_engine.write() {
                *guard = engine.clone();
            }
            runtime.set_llm_engine(engine);
            return;
        }

        let previous = llm_engine.write().ok().map(|mut guard| {
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
        let unloaded = llm_engine
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        runtime.set_llm_engine(unloaded);
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
        } else if !needs_local_stt(settings) && whisper_was_loaded {
            let _ = self.rotate_transcriber_cancel();
            self.unload_transcriber().await;
        }

        if plan.reload_llm_engine || !needs_local_llm(settings) {
            let settings = settings.clone();
            let llm_engine = Arc::clone(&self.llm_engine);
            let runtime = Arc::clone(&self.runtime);
            let defer_llm_load = plan.reload_llm_engine;
            if tokio::task::spawn_blocking(move || {
                AppContext::sync_llm_engine_owned(
                    &settings,
                    &llm_engine,
                    &runtime,
                    defer_llm_load,
                );
            })
            .await
            .is_err()
            {
                tracing::warn!("local LLM reload task failed to run on blocking pool");
            }
        }

        if llm_was_ready && !needs_local_llm(settings) {
            self.record_activity(
                app,
                ActivityLevel::Info,
                "activity.memory.llm_unloaded",
                Value::Object(Default::default()),
            );
        }
        if whisper_was_loaded && !needs_local_stt(settings) {
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
        note_dictation_activity(&self.llm_dictation_activity_ms, message_key);
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

            let settings = controller.settings().clone();
            ctx.spawn_local_stt_load_in_background(&settings);
            let ptt_streaming = settings.push_to_talk
                && controller.is_push_to_talk_active()
                && settings.ptt_hold;
            let stream_live = !controller.status().ptt_hold;
            let was_ready = controller.status().state == crate::app::state::AppState::Ready;
            let _ = controller.on_speech_started(&app_handle);
            drop(controller);

            if was_ready && settings.recording_indicator {
                crate::game_input::overlay::router::show_recording_overlay(&app_handle);
            }

            if stream_live || ptt_streaming {
                crate::injection::focus_target::capture_injection_target();
                ctx.streaming_preview.start();
            }
            if stream_live {
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

            let keep_preview = ctx.controller.try_lock().ok().is_some_and(|controller| {
                let ptt_active = controller.is_push_to_talk_active();
                if !ptt_active {
                    return false;
                }
                settings.ai_postprocess_mode().is_some()
                    || (settings.push_to_talk && settings.ptt_hold)
            });
            if !keep_preview {
                ctx.streaming_preview.stop_and_clear(&app_handle);
            }

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
        self.ptt_postprocess.reset();
        if let Ok(audio) = self.audio.lock() {
            audio.set_ptt_vad_segments_on_silence(false);
        }
    }
}
