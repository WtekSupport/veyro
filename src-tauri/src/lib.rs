//! Cross-platform stubs (game input, elevation, etc.) are mostly Windows-only; allow dead code on Linux/macOS CI.
#![cfg_attr(not(windows), allow(dead_code))]

pub mod app;
pub mod audio;
mod deeplink;
mod diagnostics;
mod error;
mod game_input;
mod hwid;
mod hotkey;
mod i18n;
mod notify;
pub mod injection;
mod network;
mod privacy;
pub mod llm;
mod setup;
pub mod settings;
pub mod text;
pub mod timed_text;
pub mod transcription;
mod tray;
mod window;
pub mod vad;

use std::sync::Arc;

use app::activity_log::{ActivityLevel, ActivityLogEntry};
use app::info::{self, AppInfo, ThirdPartyLicense};
use serde_json::json;
use app::context::AppContext;
use app::controller::{AppController, AudioAction};
use app::state::{DiagnosticsSnapshot, StatusSnapshot};
use injection::create_injector;
use network::HttpClient;
use llm::model_store as llm_model_store;
use llm::LlmEngine;
use settings::{
    clear_api_key, has_api_key, load_settings, save_api_key, save_settings, AppSettings,
    LlmModelKind, LocalSttModelKind, SettingsPatch, UiMode, WhisperModelKind,
};
use setup::HomemakerLocalSetup;
use text::dictionary::{
    ensure_dictionary_file,
};
use text::skill::{import_skill, list_skills, open_skills_folder, AiSkillInfo};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};
use transcription::{create_transcriber, model_store};

#[cfg(all(windows, feature = "local-llm"))]
fn configure_llm_dll_search(app: &AppHandle) {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

    let Ok(resource_dir) = app.path().resource_dir() else {
        return;
    };
    if !resource_dir.join("llama.dll").is_file() {
        return;
    }

    let mut wide: Vec<u16> = resource_dir.as_os_str().encode_wide().collect();
    wide.push(0);
    unsafe {
        let _ = SetDllDirectoryW(windows::core::PCWSTR(wide.as_ptr()));
    }
}

#[tauri::command]
fn get_status(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<StatusSnapshot, String> {
    ctx.inner()
        .controller
        .try_lock()
        .map(|controller| controller.status())
        .map_err(|_| "application is busy, try again".to_string())
}

pub(crate) fn apply_audio_action(
    ctx: &AppContext,
    app: &AppHandle,
    action: AudioAction,
) -> Result<(), error::AppError> {
    match action {
        AudioAction::None => Ok(()),
        AudioAction::Enable => {
            ctx.runtime.reset_cancel();
            let (device, mode, vad_config) = {
                let controller = ctx
                    .controller
                    .lock()
                    .map_err(|_| error::AppError::Internal("controller lock poisoned".into()))?;
                (
                    controller.settings().microphone_device.clone(),
                    controller.capture_mode(),
                    controller.settings().vad_config(),
                )
            };
            ctx.set_audio_callbacks_enabled(false);
            let audio_result = {
                let mut audio = ctx
                    .audio
                    .lock()
                    .map_err(|_| error::AppError::Internal("audio lock poisoned".into()))?;
                audio.set_vad_config(vad_config);
                audio.start(device, mode).map_err(error::AppError::from)
            };
            if let Err(error) = audio_result {
                ctx.set_audio_callbacks_enabled(true);
                return Err(error);
            }
            let finish_result = {
                let mut controller = ctx
                    .controller
                    .lock()
                    .map_err(|_| error::AppError::Internal("controller lock poisoned".into()))?;
                controller.finish_audio_action(app, AudioAction::Enable)
            };
            ctx.set_audio_callbacks_enabled(true);
            let result = finish_result;
            if result.is_ok() {
                hotkey::manager::HotkeyManager::register_from_settings(app);
                ctx.record_activity(
                    Some(app),
                    ActivityLevel::Info,
                    "activity.audio.capture_started",
                    json!({ "mode": capture_mode_key(mode) }),
                );
            }
            result
        }
        AudioAction::Disable => {
            ctx.cancel_pending();
            ctx.runtime.reset_cancel();
            ctx.set_audio_callbacks_enabled(false);
            let vad_join = {
                let mut audio = ctx
                    .audio
                    .lock()
                    .map_err(|_| error::AppError::Internal("audio lock poisoned".into()))?;
                audio.stop().map_err(error::AppError::from)?
            };
            join_vad_worker(vad_join);
            ctx.set_audio_callbacks_enabled(true);
            let mut controller = ctx
                .controller
                .lock()
                .map_err(|_| error::AppError::Internal("controller lock poisoned".into()))?;
            let result = controller.finish_audio_action(app, AudioAction::Disable);
            if result.is_ok() {
                ctx.record_activity(
                    Some(app),
                    ActivityLevel::Info,
                    "activity.audio.pipeline_stopped",
                    json!({}),
                );
            }
            result
        }
        AudioAction::Restart => {
            let (device, mode, vad_config) = {
                let controller = ctx
                    .controller
                    .lock()
                    .map_err(|_| error::AppError::Internal("controller lock poisoned".into()))?;
                (
                    controller.settings().microphone_device.clone(),
                    controller.capture_mode(),
                    controller.settings().vad_config(),
                )
            };
            ctx.set_audio_callbacks_enabled(false);
            let vad_join = {
                let mut audio = ctx
                    .audio
                    .lock()
                    .map_err(|_| error::AppError::Internal("audio lock poisoned".into()))?;
                let stop_join = audio.stop().map_err(error::AppError::from)?;
                audio.set_vad_config(vad_config);
                audio.start(device, mode).map_err(error::AppError::from)?;
                stop_join
            };
            join_vad_worker(vad_join);
            ctx.set_audio_callbacks_enabled(true);
            let mut controller = ctx
                .controller
                .lock()
                .map_err(|_| error::AppError::Internal("controller lock poisoned".into()))?;
            let result = controller.finish_audio_action(app, AudioAction::Restart);
            if result.is_ok() {
                hotkey::manager::HotkeyManager::register_from_settings(app);
                ctx.record_activity(
                    Some(app),
                    ActivityLevel::Info,
                    "activity.audio.pipeline_restarted",
                    json!({ "mode": capture_mode_key(mode) }),
                );
            }
            result
        }
    }
}

fn join_vad_worker(handle: Option<std::thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

pub(crate) fn spawn_prewarm_local_models_if_enabled(
    ctx: Arc<AppContext>,
    app: Option<AppHandle>,
    settings: AppSettings,
) {
    if settings.prewarm_local_models_at_startup {
        spawn_prewarm_local_models(ctx, app, settings);
    }
}

pub(crate) fn spawn_prewarm_local_models(
    ctx: Arc<AppContext>,
    app: Option<AppHandle>,
    settings: AppSettings,
) {
    let prewarm_stt = crate::app::memory::needs_local_stt(&settings)
        && crate::app::memory::selected_local_stt_ready(&settings);
    let prewarm_llm = crate::llm::model_store::needs_local_llm(&settings);

    if !prewarm_stt && !prewarm_llm {
        return;
    }

    tauri::async_runtime::spawn(async move {
        if prewarm_stt {
            match ctx.runtime.prewarm_transcriber().await {
                Ok(()) => {
                    ctx.record_activity(
                        app.as_ref(),
                        ActivityLevel::Info,
                        "activity.model.prewarm_done",
                        json!({}),
                    );
                }
                Err(error) => {
                    tracing::warn!("local STT prewarm failed: {error}");
                    ctx.record_activity(
                        app.as_ref(),
                        ActivityLevel::Warn,
                        "activity.model.prewarm_failed",
                        json!({ "error": error.to_string() }),
                    );
                }
            }
        }

        if prewarm_llm {
            if let Err(error) = LlmEngine::ensure_loaded(&settings, &ctx.llm_engine) {
                tracing::warn!("local LLM ensure_loaded before prewarm failed: {error}");
            }
            let engine = ctx
                .llm_engine
                .read()
                .map(|guard| guard.clone())
                .unwrap_or_else(|poisoned| poisoned.into_inner().clone());

            if engine.is_ready() {
                match engine.prewarm().await {
                    Ok(()) => {
                        ctx.record_activity(
                            app.as_ref(),
                            ActivityLevel::Info,
                            "activity.llm.prewarm_done",
                            json!({}),
                        );
                    }
                    Err(error) => {
                        tracing::warn!("local LLM prewarm failed: {error}");
                        ctx.record_activity(
                            app.as_ref(),
                            ActivityLevel::Warn,
                            "activity.llm.prewarm_failed",
                            json!({ "error": error }),
                        );
                    }
                }
            }
        }
    });
}

pub(crate) fn spawn_prewarm_microphone(
    ctx: Arc<AppContext>,
    app: Option<AppHandle>,
    device_id: Option<String>,
) {
    std::thread::spawn(move || {
        if let Ok(mut audio) = ctx.audio.lock() {
            match audio.prewarm(device_id) {
                Ok(()) => {
                    if audio.is_capturing() {
                        ctx.record_activity(
                            app.as_ref(),
                            ActivityLevel::Info,
                            "activity.mic.prewarm_vad",
                            json!({}),
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!("microphone prewarm failed: {error}");
                    ctx.record_activity(
                        app.as_ref(),
                        ActivityLevel::Warn,
                        "activity.mic.prewarm_failed",
                        json!({ "error": error.to_string() }),
                    );
                }
            }
        }
    });
}

pub(crate) fn apply_ptt_active(
    ctx: &AppContext,
    app: &AppHandle,
    active: bool,
) -> Result<bool, error::AppError> {
    if active {
        ctx.runtime.reset_cancel();
        ctx.begin_dictation_session(app);
        ctx.set_audio_callbacks_enabled(false);
        let vad_join = {
            let mut audio = ctx
                .audio
                .lock()
                .map_err(|_| error::AppError::Internal("audio lock poisoned".into()))?;
            audio
                .set_ptt_active(true)
                .map_err(error::AppError::from)?
        };
        join_vad_worker(vad_join);
        ctx.set_audio_callbacks_enabled(true);
        if let Ok(controller) = ctx.controller.try_lock() {
            if controller.settings().ai_postprocess_mode().is_some() {
                ctx.ptt_postprocess.begin_session();
            }
        }
        if let Ok(audio) = ctx.audio.lock() {
            let segment_on_silence = ctx
                .controller
                .try_lock()
                .ok()
                .is_some_and(|c| c.settings().push_to_talk && !c.settings().ptt_hold);
            audio.set_ptt_vad_segments_on_silence(segment_on_silence);
        }
        let capture_status = ctx.audio.lock().ok().map(|audio| {
            json!({
                "vad_alive": audio.is_capturing(),
                "stream_active": audio.is_input_stream_active(),
                "mic_level": audio.mic_level(),
            })
        });
        ctx.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.ptt.pressed",
            capture_status.clone().unwrap_or_else(|| json!({})),
        );
        if capture_status.is_some_and(|status| {
            status.get("stream_active").and_then(|v| v.as_bool()) != Some(true)
        }) {
            ctx.record_activity(
                Some(app),
                ActivityLevel::Warn,
                "activity.capture.stream_inactive",
                json!({}),
            );
        }
        return Ok(false);
    }

    let flush_rx = {
        let mut audio = ctx
            .audio
            .lock()
            .map_err(|_| error::AppError::Internal("audio lock poisoned".into()))?;
        audio.begin_ptt_release().map_err(error::AppError::from)?
    };

    complete_ptt_release(ctx, app, flush_rx)
}

pub(crate) fn complete_ptt_release(
    ctx: &AppContext,
    app: &AppHandle,
    flush_rx: Option<crossbeam_channel::Receiver<crate::audio::segment::AudioSegment>>,
) -> Result<bool, error::AppError> {
    let mut flushed = flush_rx.and_then(|rx| {
        let (segment, rx) = crate::audio::pipeline::AudioPipeline::wait_for_ptt_flush(rx);
        if let Ok(mut audio) = ctx.audio.lock() {
            audio.restore_ptt_flush_rx(rx);
        }
        segment
    });

    if flushed.is_none() {
        let deferred_ai = ctx
            .controller
            .try_lock()
            .ok()
            .and_then(|c| c.settings().ai_postprocess_mode())
            .is_some();
        flushed = ctx.audio.lock().ok().and_then(|mut audio| {
            if deferred_ai {
                audio.drain_pending_segments();
            }
            audio.poll_ptt_segment()
        });
    }

    if let Some(segment) = flushed {
        if crate::audio::preprocess::is_silent_segment(&segment) {
            if let Ok(mut audio) = ctx.audio.lock() {
                audio.drain_pending_segments();
            }
            ctx.record_activity(
                Some(app),
                ActivityLevel::Warn,
                "activity.ptt.released_silent",
                json!({ "ms": segment.duration_ms }),
            );
            return Ok(false);
        }

        let settings = match ctx.controller.try_lock() {
            Ok(controller) => controller.settings().clone(),
            Err(_) => {
                warn!("ptt release: controller busy while queueing segment, dropping flushed audio");
                if let Ok(mut audio) = ctx.audio.lock() {
                    audio.drain_pending_segments();
                }
                return Ok(false);
            }
        };

        if settings.requires_api_key_for_transcription() && !has_api_key() {
            ctx.record_activity(
                Some(app),
                ActivityLevel::Error,
                "activity.segment_no_api_key",
                json!({ "ms": segment.duration_ms }),
            );
            return Ok(false);
        }

        ctx.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.ptt.released_flushed",
            json!({ "ms": segment.duration_ms }),
        );
        match ctx.runtime.enqueue_segment(
            app.clone(),
            ctx.controller.clone(),
            segment.clone(),
            settings,
        ) {
            crate::app::runtime::EnqueueResult::Ok { .. } => {}
            crate::app::runtime::EnqueueResult::DiskQueueFull => {
                ctx.record_activity(
                    Some(app),
                    ActivityLevel::Error,
                    "activity.segment_queue_disk_full",
                    json!({ "ms": segment.duration_ms }),
                );
                if let Ok(mut audio) = ctx.audio.lock() {
                    audio.drain_pending_segments();
                }
                return Ok(false);
            }
            crate::app::runtime::EnqueueResult::PendingQueueFull => {
                ctx.record_activity(
                    Some(app),
                    ActivityLevel::Warn,
                    "activity.segment_dropped_queue_full",
                    json!({ "ms": segment.duration_ms }),
                );
                if let Ok(mut audio) = ctx.audio.lock() {
                    audio.drain_pending_segments();
                }
                return Ok(false);
            }
            crate::app::runtime::EnqueueResult::QueueClosed => {
                if let Ok(mut audio) = ctx.audio.lock() {
                    audio.drain_pending_segments();
                }
                return Ok(false);
            }
        }
        if let Ok(mut audio) = ctx.audio.lock() {
            audio.drain_pending_segments();
        }
        return Ok(true);
    }

    if let Ok(mut audio) = ctx.audio.lock() {
        audio.drain_pending_segments();
    }

    warn!("ptt release produced no audio segment");
    ctx.record_activity(
        Some(app),
        ActivityLevel::Warn,
        "activity.ptt.released_empty",
        json!({}),
    );
    Ok(false)
}

async fn apply_audio_action_async(
    ctx: Arc<AppContext>,
    app: AppHandle,
    action: AudioAction,
) -> Result<(), error::AppError> {
    if action == AudioAction::None {
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || apply_audio_action(&ctx, &app, action))
        .await
        .map_err(|error| {
            error::AppError::Internal(format!("audio worker task failed: {error}"))
        })?
}

#[tauri::command]
fn get_settings(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<AppSettings, String> {
    ctx.inner()
        .controller
        .try_lock()
        .map(|controller| controller.settings().clone())
        .map_err(|_| "application is busy, try again".to_string())
}

#[tauri::command]
async fn update_settings(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let ctx = ctx.inner().clone();
    let plan = {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        controller
            .plan_settings_update(patch)
            .map_err(|error| error.to_string())?
    };

    if plan.hotkey_spec_update {
        let hook_ptt_hold =
            plan.settings.push_to_talk && !hotkey::ptt_mode::press_to_toggle(&plan.settings);
        let config = game_input::HotkeyInstallConfig {
            hotkey: plan.settings.global_hotkey.clone(),
            #[cfg(not(windows))]
            game_mode: plan.settings.hotkey_game_mode,
            block_system: plan.settings.effective_hotkey_block_system(),
            ptt_hold: hook_ptt_hold,
        };
        if let Err(error) = game_input::update_config(&app, config) {
            warn!("hotkey spec update failed, re-registering: {error}");
            hotkey::manager::HotkeyManager::register_with_feedback_on_main(
                &app,
                &plan.settings.global_hotkey,
                plan.settings.hotkey_game_mode,
                plan.settings.effective_hotkey_block_system(),
            );
        }
    } else if plan.reregister_hotkey {
        hotkey::manager::HotkeyManager::register_with_feedback_on_main(
            &app,
            &plan.settings.global_hotkey,
            plan.settings.hotkey_game_mode,
            plan.settings.effective_hotkey_block_system(),
        );
    }

    if plan.capslock_ptt_changed {
        if let Err(error) = hotkey::capslock::set_enabled(plan.settings.capslock_ptt) {
            warn!("caps lock remap failed: {error}");
        }
    }

    #[cfg(windows)]
    #[cfg(windows)]
    game_input::hotkey_win::set_ptt_hold(
        plan.settings.push_to_talk && !hotkey::ptt_mode::press_to_toggle(&plan.settings),
    );

    apply_audio_action_async(ctx.clone(), app.clone(), plan.audio_action)
        .await
        .map_err(|error| error.to_string())?;

    if plan.audio_action == AudioAction::None {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        controller
            .finish_audio_action(&app, AudioAction::None)
            .map_err(|error| error.to_string())?;
    }

    save_settings(&plan.settings).map_err(|error| error.to_string())?;

    let reload_whisper = plan.reload_transcriber;
    let reload_llm = plan.reload_llm_engine;
    let post_audio_action = plan.audio_action;
    let post_settings = plan.settings.clone();
    let background_plan = plan.clone();
    let background_app = app.clone();
    tauri::async_runtime::spawn(async move {
        ctx.apply_memory_policy(Some(&background_app), &post_settings, &background_plan)
            .await;

        if reload_whisper {
            ctx.record_activity(
                Some(&background_app),
                ActivityLevel::Info,
                "activity.model.reloaded",
                json!({}),
            );
        }

        if reload_llm {
            ctx.record_activity(
                Some(&background_app),
                ActivityLevel::Info,
                "activity.llm.reloaded",
                json!({}),
            );
        }

        if post_audio_action == AudioAction::None {
            spawn_prewarm_microphone(
                ctx.clone(),
                Some(background_app.clone()),
                post_settings.microphone_device.clone(),
            );
            let model_identity_changed = reload_whisper || reload_llm;
            if !model_identity_changed {
                spawn_prewarm_local_models_if_enabled(
                    ctx.clone(),
                    Some(background_app),
                    post_settings,
                );
            }
        }
    });

    if plan.settings.start_on_boot {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }

    apply_window_locale(&app, plan.settings.ui_locale);
    window::configure_main_window_for_ui_mode(&app, plan.settings.ui_mode);
    tray::menu::refresh_tray_menu(&app);
    if plan.settings.recording_indicator {
        let overlay_prewarm = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            window::prewarm_recording_overlay(&overlay_prewarm);
        });
    }
    let _ = app.emit("app://settings-changed", &plan.settings);
    Ok(plan.settings)
}

#[tauri::command]
fn get_homemaker_local_setup(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<HomemakerLocalSetup, String> {
    let controller = ctx
        .controller
        .try_lock()
        .map_err(|_| "application is busy, try again".to_string())?;
    Ok(setup::get_homemaker_local_setup(controller.settings()))
}

#[tauri::command]
fn get_homemaker_hotkey_presets() -> Vec<String> {
    setup::homemaker_hotkey_presets()
        .iter()
        .map(|preset| (*preset).to_string())
        .collect()
}

#[tauri::command]
async fn recover_engine(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<StatusSnapshot, String> {
    let ctx = ctx.inner().clone();
    ctx.cancel_pending();
    ctx.runtime.reset_cancel();

    let action = {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        controller
            .recover_from_error(&app)
            .map_err(|error| error.to_string())?
    };

    apply_audio_action_async(ctx.clone(), app.clone(), action)
        .await
        .map_err(|error| error.to_string())?;

    let settings = {
        let controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        controller.settings().clone()
    };
    ctx.reload_transcriber(&settings).await;
    spawn_prewarm_microphone(
        ctx.clone(),
        Some(app.clone()),
        settings.microphone_device.clone(),
    );
    spawn_prewarm_local_models_if_enabled(ctx.clone(), Some(app.clone()), settings.clone());

    ctx.controller
        .lock()
        .map(|controller| controller.status())
        .map_err(|_| "application controller lock poisoned".to_string())
}

#[tauri::command]
fn get_devices() -> Result<Vec<String>, String> {
    audio::list_devices()
        .map(|devices| devices.into_iter().map(|device| device.name).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn prewarm_microphone(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
    device_id: Option<String>,
) -> Result<(), String> {
    spawn_prewarm_microphone(ctx.inner().clone(), Some(app), device_id);
    Ok(())
}

#[tauri::command]
async fn force_unload_local_models(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<(), String> {
    let ctx = ctx.inner().clone();
    if ctx.runtime.pending_count() > 0 {
        return Err("models_unload_busy".to_string());
    }
    ctx.force_unload_loaded_models(Some(&app)).await;
    Ok(())
}

#[tauri::command]
fn prewarm_local_models(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<(), String> {
    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    spawn_prewarm_local_models(ctx.inner().clone(), Some(app), settings);
    Ok(())
}

#[tauri::command]
fn set_api_key(key: String) -> Result<(), String> {
    save_api_key(&key).map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_api_key_command() -> Result<(), String> {
    clear_api_key().map_err(|error| error.to_string())
}

#[tauri::command]
fn has_api_key_command() -> bool {
    has_api_key()
}

#[tauri::command]
fn get_diagnostics(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> DiagnosticsSnapshot {
    diagnostics::collect_diagnostics(ctx.inner(), &app)
}

#[tauri::command]
fn ensure_mic_monitor(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<(), String> {
    let device = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application is busy, try again".to_string())?
        .settings()
        .microphone_device
        .clone();

    let mut audio = ctx
        .inner()
        .audio
        .lock()
        .map_err(|_| "application is busy, try again".to_string())?;
    audio
        .ensure_level_monitor(device)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_mic_level(ctx: tauri::State<'_, Arc<AppContext>>) -> u32 {
    use crate::audio::level::mic_level_percent;

    mic_level_percent(&ctx.inner().mic_level).min(100)
}

#[derive(serde::Serialize)]
struct MicMonitorSnapshot {
    level_percent: u8,
    speech_active: bool,
    effective_threshold_percent: u8,
    history: Vec<u8>,
}

#[tauri::command]
fn get_mic_monitor_snapshot(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<MicMonitorSnapshot, String> {
    use crate::audio::level::mic_level_percent;

    let ctx = ctx.inner();
    let (speech_active, effective_threshold) = {
        let controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        (
            ctx.audio
                .lock()
                .map(|audio| audio.is_speech_active())
                .unwrap_or(false),
            controller.settings().effective_vad_threshold_percent(),
        )
    };

    Ok(MicMonitorSnapshot {
        level_percent: mic_level_percent(&ctx.mic_level).min(100) as u8,
        speech_active,
        effective_threshold_percent: effective_threshold,
        history: ctx.mic_monitor.history(),
    })
}

#[tauri::command]
async fn calibrate_vad_threshold(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<u8, String> {
    let ctx = ctx.inner().clone();
    let device = {
        let controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        controller.settings().microphone_device.clone()
    };

    {
        let mut audio = ctx
            .audio
            .lock()
            .map_err(|_| "application is busy, try again".to_string())?;
        audio
            .ensure_level_monitor(device)
            .map_err(|error| error.to_string())?;
    }

    ctx.mic_monitor.clear();
    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

    let samples = ctx.mic_monitor.history();
    let threshold = crate::vad::threshold::compute_auto_threshold_percent(&samples);

    let patch = SettingsPatch {
        vad_threshold_mode: Some(crate::settings::VadThresholdMode::Auto),
        vad_auto_threshold_percent: Some(threshold),
        ..Default::default()
    };

    let plan = {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        controller
            .plan_settings_update(patch)
            .map_err(|error| error.to_string())?
    };

    apply_audio_action_async(ctx.clone(), app.clone(), plan.audio_action)
        .await
        .map_err(|error| error.to_string())?;

    save_settings(&plan.settings).map_err(|error| error.to_string())?;

    let _ = app.emit("app://settings-changed", &plan.settings);

    Ok(threshold)
}

#[tauri::command]
fn get_activity_log(ctx: tauri::State<'_, Arc<AppContext>>) -> Vec<ActivityLogEntry> {
    ctx.inner().activity_log.snapshot()
}

#[derive(serde::Serialize)]
struct WhisperModelStatus {
    path: String,
    exists: bool,
}

#[tauri::command]
fn get_whisper_model_status(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<WhisperModelStatus, String> {
    let settings = settings_for_models_dir(ctx.inner())?;
    let variant = crate::settings::normalize_variant(crate::settings::LocalSttVariant::new(
        settings.local_stt_family,
        settings.local_stt_quant,
    ));
    let path =
        transcription::local_stt_model_store::resolve_model_bundle(&settings).map_err(|e| e.to_string())?;
    Ok(WhisperModelStatus {
        path: path.display().to_string(),
        exists: transcription::local_stt_model_store::bundle_ready_for_settings(&settings, variant),
    })
}

#[tauri::command]
fn list_transcription_languages() -> Vec<transcription::TranscriptionLanguage> {
    transcription::list_transcription_languages()
}

#[tauri::command]
fn list_whisper_models(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<Vec<model_store::WhisperModelInfo>, String> {
    let settings = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    model_store::list_models(&settings).map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_whisper_model(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
    model: WhisperModelKind,
) -> Result<String, String> {
    use crate::app::events::WHISPER_MODEL_DOWNLOAD_PROGRESS;

    let settings = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    let http = model_store::download_client().map_err(|error| error.to_string())?;
    ctx.inner().record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.model.download_started",
        json!({ "model": model.file_name() }),
    );

    let app_for_progress = app.clone();
    let path = model_store::download_model(&http, &settings, model, |progress| {
        let _ = app_for_progress.emit(WHISPER_MODEL_DOWNLOAD_PROGRESS, progress);
    })
    .await
    .inspect_err(|error| {
        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Error,
            "activity.model.download_failed",
            json!({ "error": error.clone() }),
        );
    })?;

    ctx.inner().record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.model.download_done",
        json!({ "path": path.display().to_string() }),
    );

    if settings.local_stt_model.whisper_kind() == Some(model) {
        ctx.inner().reload_transcriber(&settings).await;
        spawn_prewarm_local_models(ctx.inner().clone(), Some(app.clone()), settings);
    }

    Ok(path.display().to_string())
}

#[tauri::command]
fn list_local_stt_models(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<Vec<transcription::local_stt_model_store::LocalSttModelInfo>, String> {
    let settings = settings_for_models_dir(ctx.inner())?;
    transcription::local_stt_model_store::list_models(&settings).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_local_stt_families() -> Vec<transcription::local_stt_model_store::LocalSttFamilyInfo> {
    transcription::local_stt_model_store::list_families()
}

#[tauri::command]
fn describe_local_stt_variant(
    ctx: tauri::State<'_, Arc<AppContext>>,
    family: crate::settings::LocalSttFamily,
    quant: crate::settings::LocalSttQuant,
) -> Result<transcription::local_stt_model_store::LocalSttVariantInfo, String> {
    let settings = settings_for_models_dir(ctx.inner())?;
    let variant = crate::settings::normalize_variant(crate::settings::LocalSttVariant::new(
        family, quant,
    ));
    transcription::local_stt_model_store::describe_variant(&settings, variant)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_local_stt_model(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
    model: LocalSttModelKind,
) -> Result<String, String> {
    let variant = crate::settings::migrate_from_legacy_stt_model(model);
    download_local_stt_variant(app, ctx, variant.family, variant.quant).await
}

#[tauri::command]
async fn download_local_stt_variant(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
    family: crate::settings::LocalSttFamily,
    quant: crate::settings::LocalSttQuant,
) -> Result<String, String> {
    use crate::app::events::WHISPER_MODEL_DOWNLOAD_PROGRESS;

    let settings = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    let variant = crate::settings::LocalSttVariant::new(family, quant);
    let http = transcription::local_stt_model_store::download_client_for_stt()
        .map_err(|error| error.to_string())?;
    ctx.inner().record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.model.download_started",
        json!({ "model": variant.as_api_id() }),
    );

    let app_for_progress = app.clone();
    let path = transcription::local_stt_model_store::download_model(&http, &settings, variant, |progress| {
        let _ = app_for_progress.emit(WHISPER_MODEL_DOWNLOAD_PROGRESS, progress);
    })
    .await
    .inspect_err(|error| {
        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Error,
            "activity.model.download_failed",
            json!({ "error": error.clone() }),
        );
    })?;

    ctx.inner().record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.model.download_done",
        json!({ "path": path.display().to_string() }),
    );

    if settings.local_stt_variant() == variant {
        ctx.inner().reload_transcriber(&settings).await;
        spawn_prewarm_local_models(ctx.inner().clone(), Some(app.clone()), settings);
    }

    Ok(path.display().to_string())
}

fn settings_for_models_dir(ctx: &AppContext) -> Result<crate::settings::AppSettings, String> {
    let mut settings = if let Ok(controller) = ctx.controller.try_lock() {
        controller.settings().clone()
    } else {
        crate::settings::load_settings().map_err(|error| error.to_string())?
    };
    settings.repair_local_stt_selection();
    Ok(settings)
}

#[tauri::command]
fn get_whisper_models_dir(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<String, String> {
    let settings = settings_for_models_dir(ctx.inner())?;
    model_store::resolve_models_dir(&settings)
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn pick_whisper_models_dir() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string()))
}

#[derive(serde::Serialize)]
struct LlmModelStatus {
    path: String,
    exists: bool,
}

#[tauri::command]
fn get_llm_model_status(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<LlmModelStatus, String> {
    let settings = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    let path = llm_model_store::resolve_model_path(&settings).map_err(|error| error.to_string())?;
    Ok(LlmModelStatus {
        path: path.display().to_string(),
        exists: llm_model_store::model_exists(&path),
    })
}

#[tauri::command]
fn list_llm_models(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<Vec<llm_model_store::LlmModelInfo>, String> {
    let settings = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    llm_model_store::list_models(&settings).map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_llm_model(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
    model: LlmModelKind,
) -> Result<String, String> {
    use crate::app::events::LLM_MODEL_DOWNLOAD_PROGRESS;
    use crate::settings::UiLocale;

    let settings = ctx
        .inner()
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    if !model.visible_for_ui_locale(settings.ui_locale) {
        return Err(if settings.ui_locale == UiLocale::Ru {
            "model is not available for the current UI locale".to_string()
        } else {
            "Russian-tuned LLM model is only available when the UI locale is Russian".to_string()
        });
    }

    let http = model_store::download_client().map_err(|error| error.to_string())?;
    ctx.inner().record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.llm.download_started",
        json!({ "model": model.file_name() }),
    );

    let app_for_progress = app.clone();
    let path = llm_model_store::download_model(&http, &settings, model, |progress| {
        let _ = app_for_progress.emit(LLM_MODEL_DOWNLOAD_PROGRESS, progress);
    })
    .await
    .inspect_err(|error| {
        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Error,
            "activity.llm.download_failed",
            json!({ "error": error.clone() }),
        );
    })?;

    ctx.inner().record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.llm.download_done",
        json!({ "path": path.display().to_string() }),
    );

    if settings.local_llm_model == model {
        ctx.inner().sync_llm_engine(&settings);
        if crate::app::memory::needs_llm_prewarm(&settings) {
            spawn_prewarm_local_models(ctx.inner().clone(), Some(app.clone()), settings);
        }
    }

    Ok(path.display().to_string())
}

#[tauri::command]
fn get_llm_models_dir(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<String, String> {
    let settings = settings_for_models_dir(ctx.inner())?;
    llm_model_store::resolve_models_dir(&settings)
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn pick_llm_models_dir() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string()))
}

#[derive(serde::Serialize)]
struct SileroTeModelStatus {
    path: String,
    exists: bool,
    size_mb: u32,
}

#[tauri::command]
fn get_silero_te_model_status(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<SileroTeModelStatus, String> {
    #[cfg(not(feature = "silero-te"))]
    {
        let _ = ctx;
        return Ok(SileroTeModelStatus {
            path: String::new(),
            exists: false,
            size_mb: 0,
        });
    }
    #[cfg(feature = "silero-te")]
    {
        let settings = settings_for_models_dir(ctx.inner())?;
        let status = crate::text::silero_te::model_store::status(&settings)
            .map_err(|error| error.to_string())?;
        Ok(SileroTeModelStatus {
            path: status.path,
            exists: status.exists,
            size_mb: status.size_mb,
        })
    }
}

#[tauri::command]
async fn download_silero_te_model(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<String, String> {
    #[cfg(not(feature = "silero-te"))]
    {
        let _ = (app, ctx);
        return Err("Silero TE is not compiled into this build".to_string());
    }
    #[cfg(feature = "silero-te")]
    {
        use crate::app::events::SILERO_TE_DOWNLOAD_PROGRESS;

        let settings = settings_for_models_dir(ctx.inner())?;

        let http = crate::text::silero_te::model_store::download_http_client()
            .map_err(|error| error.to_string())?;
        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.silero_te.download_started",
            json!({}),
        );

        let app_for_progress = app.clone();
        let _ = app_for_progress.emit(
            SILERO_TE_DOWNLOAD_PROGRESS,
            transcription::model_store::DownloadProgress::new(0, None),
        );
        let path = crate::text::silero_te::model_store::download_assets(&http, &settings, |progress| {
            let _ = app_for_progress.emit(SILERO_TE_DOWNLOAD_PROGRESS, progress);
        })
        .await
        .inspect_err(|error| {
            ctx.inner().record_activity(
                Some(&app),
                ActivityLevel::Error,
                "activity.silero_te.download_failed",
                json!({ "error": error.clone() }),
            );
        })?;

        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.silero_te.download_done",
            json!({ "path": path.display().to_string() }),
        );

        if let Ok(engine) = crate::text::silero_te::SileroTeEngine::global().lock() {
            engine.unload();
        }

        Ok(path.display().to_string())
    }
}

#[derive(serde::Serialize)]
struct SileroVadModelStatus {
    path: String,
    exists: bool,
    size_mb: u32,
}

#[tauri::command]
fn get_silero_vad_model_status(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<SileroVadModelStatus, String> {
    #[cfg(not(feature = "vad-silero"))]
    {
        let _ = ctx;
        return Ok(SileroVadModelStatus {
            path: String::new(),
            exists: false,
            size_mb: 0,
        });
    }
    #[cfg(feature = "vad-silero")]
    {
        let settings = settings_for_models_dir(ctx.inner())?;
        let status = crate::vad::silero_model::status(&settings)
            .map_err(|error| error.to_string())?;
        Ok(SileroVadModelStatus {
            path: status.path,
            exists: status.exists,
            size_mb: status.size_mb,
        })
    }
}

#[tauri::command]
async fn download_silero_vad_model(
    app: AppHandle,
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<String, String> {
    #[cfg(not(feature = "vad-silero"))]
    {
        let _ = (app, ctx);
        return Err("Silero VAD is not compiled into this build".to_string());
    }
    #[cfg(feature = "vad-silero")]
    {
        use crate::app::events::SILERO_VAD_DOWNLOAD_PROGRESS;

        let settings = ctx
            .inner()
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?
            .settings()
            .clone();

        let http = crate::vad::silero_model::download_http_client().map_err(|error| error.to_string())?;
        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.silero_vad.download_started",
            json!({}),
        );

        let app_for_progress = app.clone();
        let path = crate::vad::silero_model::download_model(&http, &settings, |progress| {
            let _ = app_for_progress.emit(SILERO_VAD_DOWNLOAD_PROGRESS, progress);
        })
        .await
        .inspect_err(|error| {
            ctx.inner().record_activity(
                Some(&app),
                ActivityLevel::Error,
                "activity.silero_vad.download_failed",
                json!({ "error": error.clone() }),
            );
        })?;

        ctx.inner().record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.silero_vad.download_done",
            json!({ "path": path.display().to_string() }),
        );

        Ok(path.display().to_string())
    }
}

#[tauri::command]
fn list_ai_skills() -> Result<Vec<AiSkillInfo>, String> {
    list_skills().map_err(|error| error.to_string())
}

#[tauri::command]
fn open_ai_skills_folder() -> Result<(), String> {
    open_skills_folder().map_err(|error| error.to_string())
}

#[tauri::command]
fn import_ai_skill(from_path: String) -> Result<AiSkillInfo, String> {
    import_skill(&from_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_skill_import_flow_snapshot() -> Option<app::events::SkillImportFlowPayload> {
    deeplink::get_skill_import_flow_snapshot()
}

#[tauri::command]
fn confirm_deeplink_skill_import() {
    deeplink::confirm_deeplink_skill_import();
}

#[tauri::command]
fn cancel_deeplink_skill_import(app: AppHandle) {
    deeplink::cancel_deeplink_skill_import(&app);
}

#[tauri::command]
fn skill_import_ui_ready(app: AppHandle) {
    deeplink::replay_skill_import_flow(&app);
}

#[tauri::command]
fn pick_and_import_ai_skill() -> Result<AiSkillInfo, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Markdown", &["md"])
        .pick_file()
    else {
        return Err("skill_import_cancelled".to_string());
    };
    import_skill(&path.to_string_lossy()).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    info::app_info()
}

#[tauri::command]
fn get_third_party_licenses() -> Vec<ThirdPartyLicense> {
    info::third_party_licenses()
}

#[tauri::command]
async fn open_about_window(app: AppHandle) -> Result<(), String> {
    if window::is_initializing(&app) {
        let handle = app.clone();
        window::await_on_main_thread(&app, move || {
            window::show_init_window(&handle);
        })
        .await?;
        return Ok(());
    }

    let handle = app.clone();
    window::await_on_main_thread(&app, move || window::show_about_window(&handle)).await?
}

#[tauri::command]
fn get_dictionary_path(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<String, String> {
    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    crate::settings::resolve_dictionary_file_path(&settings)
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_data_storage_dir(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<String, String> {
    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    crate::settings::resolve_data_storage_root(&settings)
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn pick_data_storage_dir() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
fn open_data_storage_folder(ctx: tauri::State<'_, Arc<AppContext>>) -> Result<(), String> {
    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    let root = crate::settings::resolve_data_storage_root(&settings)
        .map_err(|error| error.to_string())?;
    crate::settings::ensure_data_storage_layout(&root).map_err(|error| error.to_string())?;
    crate::text::dictionary::open_folder(&root).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_transcription_dictionary_folder(
    ctx: tauri::State<'_, Arc<AppContext>>,
) -> Result<(), String> {
    open_data_storage_folder(ctx)
}

#[tauri::command]
fn pick_transcription_dictionary() -> Result<Option<String>, String> {
    pick_data_storage_dir()
}

#[tauri::command]
fn clear_activity_log(app: AppHandle, ctx: tauri::State<'_, Arc<AppContext>>) -> Result<(), String> {
    let ctx = ctx.inner();
    ctx.activity_log.clear();
    ctx.record_activity(
        Some(&app),
        ActivityLevel::Info,
        "activity.cleared",
        json!({}),
    );
    Ok(())
}

fn capture_mode_key(mode: crate::audio::capture::CaptureMode) -> &'static str {
    use crate::audio::capture::CaptureMode;
    match mode {
        CaptureMode::PushToTalk => "ptt",
        CaptureMode::Continuous => "continuous",
    }
}

fn apply_window_locale(app: &AppHandle, locale: settings::UiLocale) {
    let app_name = i18n::translate(locale, "app.title", &[]);
    let title = info::main_window_title(&app_name, locale);
    if let Some(window) = app.get_webview_window(window::SETTINGS_WINDOW_LABEL) {
        let _ = window.set_title(&title);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    deeplink::prepare_windows_deeplink_launch();

    let (mut settings, settings_loaded) = match load_settings() {
        Ok(settings) => (settings, true),
        Err(error) => {
            eprintln!("failed to load settings, using in-memory defaults (config not overwritten): {error}");
            (AppSettings::default(), false)
        }
    };
    settings.enabled = true;
    if settings_loaded {
        let _ = save_settings(&settings);
    }

    diagnostics::init_logging(&settings.log_level);
    let _ = text::skill::ensure_skills_dir();
    let _ = ensure_dictionary_file();

    let injector = create_injector();
    let cancel = CancellationToken::new();
    let http = HttpClient::new().expect("failed to create HTTP client");
    let transcriber_cancel = cancel.clone();
    let transcriber = create_transcriber(&settings, http.inner().clone(), transcriber_cancel.clone());
    let controller = AppController::new(settings.clone(), injector.clone());
    let context = Arc::new(AppContext::new(
        controller,
        transcriber,
        injector,
        http.inner().clone(),
        transcriber_cancel,
        &settings,
    ));

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let urls = deeplink::veyro_urls_from_cli_args(argv);
            if urls.is_empty() {
                window::show_settings_window(app);
            } else {
                deeplink::handle_skill_import_urls(app, urls);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .manage(context.clone())
        .invoke_handler(tauri::generate_handler![
            get_status,
            recover_engine,
            get_settings,
            update_settings,
            get_devices,
            prewarm_microphone,
            prewarm_local_models,
            force_unload_local_models,
            set_api_key,
            clear_api_key_command,
            has_api_key_command,
            get_diagnostics,
            ensure_mic_monitor,
            get_mic_level,
            get_mic_monitor_snapshot,
            calibrate_vad_threshold,
            get_activity_log,
            clear_activity_log,
            get_whisper_model_status,
            list_transcription_languages,
            list_whisper_models,
            list_local_stt_models,
            list_local_stt_families,
            describe_local_stt_variant,
            download_local_stt_variant,
            download_whisper_model,
            download_local_stt_model,
            get_whisper_models_dir,
            pick_whisper_models_dir,
            get_llm_model_status,
            list_llm_models,
            download_llm_model,
            get_llm_models_dir,
            pick_llm_models_dir,
            get_silero_te_model_status,
            download_silero_te_model,
            get_silero_vad_model_status,
            download_silero_vad_model,
            list_ai_skills,
            open_ai_skills_folder,
            import_ai_skill,
            get_skill_import_flow_snapshot,
            confirm_deeplink_skill_import,
            cancel_deeplink_skill_import,
            skill_import_ui_ready,
            pick_and_import_ai_skill,
            get_dictionary_path,
            get_data_storage_dir,
            pick_data_storage_dir,
            open_data_storage_folder,
            open_transcription_dictionary_folder,
            pick_transcription_dictionary,
            get_app_info,
            get_third_party_licenses,
            open_about_window,
            get_homemaker_local_setup,
            get_homemaker_hotkey_presets,
            diagnostics::resource_stats::set_resource_stats_enabled,
            diagnostics::resource_stats::get_app_stats,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            diagnostics::start_stats_loop(handle.clone());

            notify::init_platform();

            #[cfg(desktop)]
            {
                if let Err(error) = app.handle().plugin(
                    tauri_plugin_updater::Builder::new().build(),
                ) {
                    warn!("updater plugin init failed: {error}");
                }
            }

            #[cfg(all(windows, feature = "local-llm"))]
            configure_llm_dll_search(&handle);

            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(error) = app.deep_link().register_all() {
                    warn!("deep link registration failed: {error}");
                }
            }

            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let deep_link_app = handle.clone();
                app.deep_link().on_open_url(move |event| {
                    let urls = event
                        .urls()
                        .iter()
                        .map(|url| url.to_string())
                        .collect::<Vec<_>>();
                    deeplink::handle_skill_import_urls(&deep_link_app, urls);
                });

                let mut startup_urls: Vec<String> = deeplink::veyro_urls_from_cli_args(
                    std::env::args().skip(1),
                );
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    startup_urls.extend(urls.iter().map(|url| url.to_string()));
                }
                startup_urls.sort();
                startup_urls.dedup();
                if !startup_urls.is_empty() {
                    deeplink::handle_skill_import_urls(&handle, startup_urls);
                }
            }

            if settings.capslock_ptt {
                if let Err(error) = hotkey::capslock::set_enabled(true) {
                    warn!("caps lock remap failed: {error}");
                }
            }

            context.wire_audio_callbacks(&handle);
            crate::app::model_idle::start_model_idle_watchdog(handle.clone(), context.clone());
            tray::setup_tray(&handle)?;
            crate::game_input::report_startup_elevation(&handle);
            apply_window_locale(&handle, settings.ui_locale);

            if !crate::llm::model_store::needs_local_llm(&settings) {
                context.sync_llm_engine(&settings);
            }

            if let Ok(resource_dir) = handle.path().resource_dir() {
                let _ = text::skill::seed_skills_from_resources(&resource_dir);
            }
            text::skill::start_skills_watcher(handle.clone());

            if settings.start_on_boot {
                let _ = app.handle().autolaunch().enable();
            }

            let ctx = context.clone();
            let startup_handle = handle.clone();
            let startup_settings = settings.clone();
            std::thread::spawn(move || {
                if let Err(error) = apply_audio_action(&ctx, &startup_handle, AudioAction::Enable) {
                    error!("controller startup failed: {error}");
                    ctx.record_activity(
                        Some(&startup_handle),
                        ActivityLevel::Error,
                        "activity.audio.startup_failed",
                        json!({ "error": error.to_string() }),
                    );
                    if let Ok(mut controller) = ctx.controller.lock() {
                        let locale = controller.settings().ui_locale;
                        controller.report_error(&startup_handle, error);
                        notify::notify(
                            &startup_handle,
                            &i18n::translate(locale, "app.title", &[]),
                            &controller
                                .status()
                                .last_error
                                .clone()
                                .unwrap_or_default(),
                        );
                    }
                }

                spawn_prewarm_local_models_if_enabled(ctx, Some(startup_handle), startup_settings);
            });

            tray::menu::refresh_tray_menu(&handle);

            if settings.recording_indicator {
                let overlay_prewarm = handle.clone();
                let _ = handle.clone().run_on_main_thread(move || {
                    window::prewarm_recording_overlay(&overlay_prewarm);
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    if window.label() == window::SETTINGS_WINDOW_LABEL
                        || window.label() == window::ABOUT_WINDOW_LABEL
                    {
                        window::hide_settings_window(window);
                    } else if window.label() == window::SKILL_IMPORT_WINDOW_LABEL {
                        api.prevent_close();
                        let app = window.app_handle();
                        deeplink::cancel_deeplink_skill_import(&app);
                    } else if window.label() == window::INIT_WINDOW_LABEL {
                        let _ = window.hide();
                    }
                }
                tauri::WindowEvent::Focused(focused) => {
                    if !focused {
                        window::maybe_release_webviews_if_minimized(window);
                    }
                }
                tauri::WindowEvent::Resized(_) => {
                    window::maybe_release_webviews_if_minimized(window);
                    if window.is_minimized().unwrap_or(false) {
                        return;
                    }
                    let app = window.app_handle();
                    if window.label() == window::SETTINGS_WINDOW_LABEL {
                        if let Some(webview) = app.get_webview_window(window::SETTINGS_WINDOW_LABEL) {
                            let ui_mode = app
                                .try_state::<Arc<AppContext>>()
                                .and_then(|ctx| {
                                    ctx.controller
                                        .lock()
                                        .ok()
                                        .map(|controller| controller.settings().ui_mode)
                                })
                                .unwrap_or(UiMode::Homemaker);
                            window::configure_window_for_ui_mode(&webview, ui_mode);
                        }
                    } else if window.label() == window::ABOUT_WINDOW_LABEL {
                        if let Some(webview) = app.get_webview_window(window::ABOUT_WINDOW_LABEL) {
                            window::configure_about_window(&webview);
                        }
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    // Keep running in the tray after all WebViews are destroyed; allow Quit/updater restart.
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => {
                    game_input::unregister();
                    hotkey::capslock::restore_on_exit();
                }
                _ => {}
            }
        });
}
