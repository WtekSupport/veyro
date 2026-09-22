use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering},
    Arc, Mutex, RwLock,
};

use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use serde_json::Value;
use crate::app::activity_log::{ActivityLevel, ActivityLog};
use crate::app::controller::{AppController, SharedController};
use crate::app::dictation_session::DictationSession;
use crate::app::ptt_postprocess::PttPostprocessSession;
use crate::app::state::AppState;
use crate::app::events::emit_activity_log;
use crate::app::runtime::PipelineRuntime;
use crate::audio::monitor::MicMonitor;
use crate::audio::pipeline::AudioPipeline;
use crate::injection::focus_defer_buffer::FocusDeferBuffer;
use crate::injection::TextInjector;
use crate::app::controller::SettingsUpdatePlan;
use crate::app::model_idle::{dictation_activity_now_ms, note_dictation_activity};
use crate::app::memory::{collect_memory_snapshot, needs_local_stt, MemorySnapshot};
use crate::llm::LlmEngine;
use crate::llm::model_store::needs_local_llm;
use crate::settings::{has_api_key, AppSettings};
use crate::transcription::{create_transcriber, TranscriptionProvider};

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
    pub ptt_postprocess: PttPostprocessSession,
    pub dictation_session: Arc<DictationSession>,
    pub focus_defer_buffer: FocusDeferBuffer,
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
        let dictation_session = Arc::new(DictationSession::new());
        let runtime = Arc::new(PipelineRuntime::new(
            transcriber,
            injector,
            http.clone(),
            root_cancel.clone(),
            activity_log.clone(),
            Arc::clone(&llm_engine),
            Arc::clone(&llm_dictation_activity_ms),
            Arc::clone(&dictation_session),
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
            ptt_postprocess: PttPostprocessSession::new(),
            dictation_session,
            focus_defer_buffer: FocusDeferBuffer::new(),
        }
    }

    fn defer_injection_without_abort(&self) -> bool {
        self.controller
            .try_lock()
            .ok()
            .is_some_and(|c| !c.settings().abort_on_focus_loss)
    }

    fn capture_injection_target_and_sync_buffer(&self) {
        let prev_target = crate::injection::focus_target::injection_target_hwnd();
        #[cfg(windows)]
        if self.defer_injection_without_abort()
            && prev_target != 0
            && !crate::injection::focus_target::focus_target_matches()
        {
            // PTT/VAD while another window is focused: keep the original field target.
            return;
        }
        crate::injection::focus_target::capture_injection_target();
        let next_target = crate::injection::focus_target::injection_target_hwnd();
        if prev_target != 0 && next_target != 0 && prev_target != next_target {
            self.focus_defer_buffer.clear();
        }
    }

    /// Starts a new dictation session (PTT press or idle→listening VAD).
    pub fn begin_dictation_session(&self, app: &AppHandle) -> u64 {
        let session_id = self.dictation_session.begin_session();
        self.capture_injection_target_and_sync_buffer();
        self.maybe_start_focus_watch(app, session_id);
        session_id
    }

    pub fn maybe_start_focus_watch(&self, app: &AppHandle, session_id: u64) {
        let abort_on_loss = self
            .controller
            .try_lock()
            .ok()
            .is_some_and(|c| c.settings().abort_on_focus_loss);
        let app_abort = app.clone();
        if abort_on_loss {
            crate::injection::focus_watch::start_focus_watch_abort(session_id, Box::new(move || {
                let Some(ctx) = app_abort.try_state::<Arc<AppContext>>() else {
                    return;
                };
                ctx.inner().abort_dictation_on_focus_loss(&app_abort);
            }));
            return;
        }

        let app_lost = app.clone();
        let app_regained = app.clone();
        crate::injection::focus_watch::start_focus_watch_defer(
            Box::new(move || {
                let Some(ctx) = app_lost.try_state::<Arc<AppContext>>() else {
                    return;
                };
                ctx.inner().on_focus_lost_defer(&app_lost);
            }),
            Box::new(move || {
                let Some(ctx) = app_regained.try_state::<Arc<AppContext>>() else {
                    return;
                };
                ctx.inner().schedule_flush_defer_buffer(&app_regained);
            }),
        );
    }

    pub fn should_defer_injection(&self) -> bool {
        #[cfg(windows)]
        {
            let abort_on_loss = self
                .controller
                .try_lock()
                .ok()
                .is_some_and(|c| c.settings().abort_on_focus_loss);
            if abort_on_loss {
                return false;
            }
            !crate::injection::focus_target::focus_target_matches()
        }
        #[cfg(not(windows))]
        {
            let _ = self;
            false
        }
    }

    pub fn on_focus_lost_defer(&self, app: &AppHandle) {
        self.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.dictation.defer_focus_loss",
            serde_json::json!({
                "focus_hwnd": crate::injection::focus_target::injection_focus_hwnd(),
            }),
        );
    }

    pub fn schedule_flush_defer_buffer(&self, app: &AppHandle) {
        let app = app.clone();
        let ctx = app
            .try_state::<Arc<AppContext>>()
            .map(|state| state.inner().clone());
        let Some(ctx) = ctx else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            crate::app::runtime::flush_focus_defer_buffer(&app, &ctx).await;
        });
    }

    pub fn append_deferred_injection(&self, normalized: String, press_enter: bool, defer_ai: bool) {
        self.focus_defer_buffer
            .push_prepared(normalized.clone(), press_enter);
        if defer_ai {
            self.ptt_postprocess
                .record_phase1_injection(&normalized, press_enter);
        }
    }

    pub fn maybe_stop_focus_watch(&self) {
        if self.focus_defer_buffer.has_pending() {
            return;
        }
        if self.runtime.pending_count() > 0 {
            return;
        }
        if self.ptt_postprocess.has_injected_text() {
            return;
        }
        let listening = self
            .controller
            .try_lock()
            .ok()
            .is_some_and(|c| c.status().state == AppState::Listening);
        if listening {
            return;
        }
        crate::injection::focus_watch::stop_focus_watch();
    }

    pub fn abort_dictation_on_focus_loss(&self, app: &AppHandle) {
        let enabled = self
            .controller
            .try_lock()
            .ok()
            .is_some_and(|c| c.settings().abort_on_focus_loss);
        if !enabled {
            return;
        }
        let Some(aborted_id) = self.dictation_session.abort_current() else {
            return;
        };

        self.cancel_pending();

        if let Ok(mut audio) = self.audio.lock() {
            audio.drain_pending_segments();
            let _ = audio.begin_ptt_release();
        }

        if let Ok(mut controller) = self.controller.try_lock() {
            if controller.is_push_to_talk_active() {
                controller.reset_ptt_active();
            }
            let _ = controller.abort_processing(app);
        }

        crate::game_input::overlay::router::on_listening_stopped(app);
        crate::game_input::reset_toggle_capture();
        #[cfg(windows)]
        crate::game_input::hotkey_win::reset_ptt_key_state();

        self.record_activity(
            Some(app),
            ActivityLevel::Warn,
            "activity.dictation.aborted_focus_loss",
            serde_json::json!({
                "session_id": aborted_id,
                "focus_hwnd": crate::injection::focus_target::injection_focus_hwnd(),
            }),
        );

        crate::injection::focus_watch::stop_focus_watch();
        crate::tray::menu::refresh_tray_menu(app);
    }

    pub async fn unload_transcriber(&self) {
        self.runtime.unload_transcriber().await;
    }

    fn transcriber_cancel_token(&self) -> CancellationToken {
        self.transcriber_cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Cancel in-flight STT work and issue a fresh token for the next transcriber instance.
    pub(crate) fn rotate_transcriber_cancel(&self) -> CancellationToken {
        let mut guard = self
            .transcriber_cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.cancel();
        *guard = CancellationToken::new();
        guard.clone()
    }

    pub async fn reload_transcriber(&self, settings: &AppSettings) {
        let transcriber = create_transcriber(
            settings,
            self.http.clone(),
            self.transcriber_cancel_token(),
        );
        self.runtime.replace_transcriber(transcriber).await;
    }

    pub fn unload_silero_te_engine(&self) {
        if let Ok(engine) = crate::text::silero_te::SileroTeEngine::global().lock() {
            engine.unload();
        }
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

        if plan.purge_inference_memory {
            self.unload_silero_te_engine();
            self.record_activity(
                app,
                ActivityLevel::Info,
                "activity.memory.inference_purged",
                Value::Object(Default::default()),
            );
        }

        if plan.reload_transcriber {
            let _ = self.rotate_transcriber_cancel();
            self.reload_transcriber(settings).await;
        } else if !needs_local_stt(settings) && whisper_was_loaded {
            let _ = self.rotate_transcriber_cancel();
            self.reload_transcriber(settings).await;
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
        if let Ok(controller) = self.controller.lock() {
            let settings = controller.settings().clone();
            self.runtime.replay_spilled_segments(
                app.clone(),
                Arc::clone(&self.controller),
                settings,
            );
        }

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
            let backlogged = ctx.runtime.pending_count() > 0;
            if !backlogged || !settings.should_reduce_prewarm_when_backlogged() {
                ctx.spawn_local_stt_load_in_background(&settings);
            }
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
                let pending = ctx.runtime.pending_count();
                if was_ready && pending == 0 && !ptt_streaming {
                    ctx.begin_dictation_session(&app_handle);
                } else {
                    ctx.capture_injection_target_and_sync_buffer();
                    let session_id = ctx.dictation_session.current_id();
                    if session_id == 0 {
                        ctx.begin_dictation_session(&app_handle);
                    } else {
                        ctx.maybe_start_focus_watch(&app_handle, session_id);
                    }
                }
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

            match ctx.runtime.enqueue_segment(
                app_handle.clone(),
                ctx.controller.clone(),
                segment.clone(),
                settings,
            ) {
                crate::app::runtime::EnqueueResult::Ok { .. } => {
                    ctx.record_activity(
                        Some(&app_handle),
                        ActivityLevel::Info,
                        "activity.vad.segment_ended",
                        serde_json::json!({ "ms": segment.duration_ms }),
                    );
                }
                crate::app::runtime::EnqueueResult::DiskQueueFull => {
                    ctx.record_activity(
                        Some(&app_handle),
                        ActivityLevel::Error,
                        "activity.segment_queue_disk_full",
                        serde_json::json!({ "ms": segment.duration_ms }),
                    );
                }
                crate::app::runtime::EnqueueResult::PendingQueueFull => {
                    ctx.record_activity(
                        Some(&app_handle),
                        ActivityLevel::Warn,
                        "activity.segment_dropped_queue_full",
                        serde_json::json!({ "ms": segment.duration_ms }),
                    );
                }
                crate::app::runtime::EnqueueResult::QueueClosed => {
                    ctx.record_activity(
                        Some(&app_handle),
                        ActivityLevel::Warn,
                        "activity.segment_dropped_busy",
                        Value::Object(Default::default()),
                    );
                }
            }
        });

        match self.audio.lock() {
            Ok(mut audio) => {
                audio.set_callbacks(on_speech_started, on_speech_ended);
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
        if let Ok(controller) = self.controller.try_lock() {
            self.runtime.clear_spill_queue(controller.settings());
        }
        self.root_cancel.cancel();
        self.ptt_postprocess.reset();
        self.focus_defer_buffer.clear();
        if let Ok(audio) = self.audio.lock() {
            audio.set_ptt_vad_segments_on_silence(false);
        }
    }
}
