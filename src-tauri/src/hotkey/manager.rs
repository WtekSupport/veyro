use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::app::controller::HotkeyPlan;
use crate::app::events::emit_listening_stopped;
use crate::app::state::AppState;
use crate::audio::segment::AudioSegment;
use crate::audio::feedback;
use crate::error::AppError;
use crate::game_input::{self, HotkeyInstallConfig};
use crate::i18n::{self, state_label};
use crate::settings::UiLocale;
use crate::hotkey::normalize::normalize_hotkey;
use crate::hotkey::ptt_mode::press_to_toggle;

pub struct HotkeyManager;

impl HotkeyManager {
    pub fn register_from_settings(app: &AppHandle) {
        let Some((hotkey, game_mode, block_system)) = app
            .try_state::<Arc<AppContext>>()
            .and_then(|ctx| {
                ctx.inner()
                    .controller
                    .try_lock()
                    .ok()
                    .map(|controller| {
                        let settings = controller.settings();
                        (
                            settings.global_hotkey.clone(),
                            settings.hotkey_game_mode,
                            settings.effective_hotkey_block_system(),
                        )
                    })
            })
        else {
            return;
        };

        Self::register_with_feedback_on_main(app, &hotkey, game_mode, block_system);
        Self::sync_ptt_hold_from_settings(app);
    }

    fn sync_ptt_hold_from_settings(_app: &AppHandle) {
        #[cfg(windows)]
        {
            let ptt_hold = _app
                .try_state::<Arc<AppContext>>()
                .and_then(|ctx| {
                    ctx.inner()
                        .controller
                        .try_lock()
                        .ok()
                        .map(|controller| {
                            let settings = controller.settings();
                            settings.push_to_talk && !press_to_toggle(settings)
                        })
                })
                .unwrap_or(true);
            crate::game_input::hotkey_win::set_ptt_hold(ptt_hold);
        }
    }

    pub fn register_with_feedback_on_main(
        app: &AppHandle,
        hotkey: &str,
        game_mode: bool,
        block_system: bool,
    ) {
        let app = app.clone();
        let hotkey = hotkey.to_string();
        std::thread::spawn(move || {
            Self::register_with_feedback(&app, &hotkey, game_mode, block_system);
        });
    }

    pub fn register(
        app: &AppHandle,
        hotkey: &str,
        game_mode: bool,
        block_system: bool,
    ) -> Result<(), AppError> {
        #[cfg(windows)]
        let _ = game_mode;

        let ptt_hold = app
            .try_state::<Arc<AppContext>>()
            .and_then(|ctx| {
                ctx.inner()
                    .controller
                    .try_lock()
                    .ok()
                    .map(|controller| {
                        let settings = controller.settings();
                        settings.push_to_talk && !press_to_toggle(settings)
                    })
            })
            .unwrap_or(true);

        let config = HotkeyInstallConfig {
            hotkey: hotkey.to_string(),
            #[cfg(not(windows))]
            game_mode,
            block_system,
            ptt_hold,
        };
        game_input::register(app, config).map(|_| ())
    }

    pub fn register_with_feedback(
        app: &AppHandle,
        hotkey: &str,
        game_mode: bool,
        block_system: bool,
    ) {
        let normalized = normalize_hotkey(hotkey);
        let locale = app_locale(app);
        match Self::register(app, hotkey, game_mode, block_system) {
            Ok(()) => {
                Self::sync_ptt_hold_from_settings(app);
                let app_prewarm = app.clone();
                let _ = app.run_on_main_thread(move || {
                    crate::window::prewarm_recording_overlay(&app_prewarm);
                });
                if hotkey_conflicts_with_ide(&normalized) {
                    crate::notify::notify(
                        app,
                        &i18n::translate(locale, "notify.hotkey_title", &[]),
                        &i18n::translate(
                            locale,
                            "notify.hotkey_conflict",
                            &[("hotkey", &normalized)],
                        ),
                    );
                }
            }
            Err(error) => {
                warn!("hotkey registration failed: {error}");
                crate::notify::notify(
                    app,
                    &i18n::translate(locale, "notify.hotkey_failed_title", &[]),
                    &i18n::translate(
                        locale,
                        "notify.hotkey_failed",
                        &[("error", &error.to_string())],
                    ),
                );
            }
        }
    }
}

fn app_locale(app: &AppHandle) -> UiLocale {
    app.try_state::<Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.inner()
                .controller
                .try_lock()
                .ok()
                .map(|c| c.settings().ui_locale)
        })
        .unwrap_or(UiLocale::En)
}

fn log_ptt_trace(app: &AppHandle, trace: &'static str, detail: &str) {
    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        info!(ptt_trace = trace, detail, "ptt (no app context)");
        return;
    };
    let toggle = game_input::toggle_capture_state();
    let snapshot = ctx.inner().controller.try_lock().ok().map(|controller| {
        let settings = controller.settings();
        (
            controller.status().state,
            settings.ptt_hold,
            press_to_toggle(settings),
            normalize_hotkey(&settings.global_hotkey),
            settings.push_to_talk,
        )
    });
    if let Some((state, ptt_hold, toggle_mode, hotkey, push_to_talk)) = snapshot {
        info!(
            ptt_trace = trace,
            detail,
            toggle = toggle.label(),
            app_state = ?state,
            ptt_hold,
            press_to_toggle = toggle_mode,
            hotkey = %hotkey,
            push_to_talk,
        );
    } else {
        info!(
            ptt_trace = trace,
            detail,
            toggle = toggle.label(),
            "ptt (controller lock busy)"
        );
    }
}

fn hotkey_conflicts_with_ide(normalized: &str) -> bool {
    matches!(
        normalized.to_ascii_lowercase().as_str(),
        "ctrl+space" | "cmdorctrl+space" | "control+space"
    )
}

pub(crate) fn spawn_ptt_press(app: AppHandle, ctx: Arc<AppContext>) {
    log_ptt_trace(&app, "spawn_ptt_press", "worker started");
    std::thread::spawn(move || {
        let allowed = ctx.controller.lock().ok().is_some_and(|controller| {
            matches!(controller.status().state, AppState::Ready)
                || matches!(
                    crate::game_input::toggle_capture_state(),
                    crate::game_input::ToggleCapture::Starting
                )
        });
        if !allowed {
            log_ptt_trace(&app, "spawn_ptt_press", "aborted (capture no longer allowed)");
            let _ = ctx.controller.lock().map(|mut c| c.reset_ptt_active());
            crate::game_input::reset_toggle_capture();
            #[cfg(windows)]
            crate::game_input::hotkey_win::reset_ptt_key_state();
            return;
        }

        let settings = ctx
            .controller
            .lock()
            .ok()
            .map(|controller| controller.settings().clone());
        let recording_indicator = settings
            .as_ref()
            .is_some_and(|s| s.recording_indicator);
        let suppress_ptt_toasts = settings
            .as_ref()
            .is_some_and(|s| s.suppress_ptt_toasts());
        if let Some(ref settings) = settings {
            ctx.spawn_local_stt_load_in_background(settings);
        }

        let _ptt_guard = ctx.ptt_lock.lock().ok();

        if let Err(error) = crate::apply_ptt_active(&ctx, &app, true) {
            log_ptt_trace(&app, "spawn_ptt_press", "apply_ptt_active failed");
            ctx.ptt_postprocess.reset();
            if let Ok(audio) = ctx.audio.lock() {
                audio.set_ptt_vad_segments_on_silence(false);
            }
            warn!("hotkey ptt press failed: {error}");
            let locale = app_locale(&app);
            crate::notify::notify(
                &app,
                &i18n::translate(locale, "notify.error_title", &[]),
                &i18n::translate(
                    locale,
                    "notify.capture_start_failed",
                    &[("error", &error.to_string())],
                ),
            );
            let _ = ctx.controller.lock().map(|mut c| {
                c.reset_ptt_active();
            });
            crate::game_input::reset_toggle_capture();
            #[cfg(windows)]
            crate::game_input::hotkey_win::reset_ptt_key_state();
            return;
        }
        if matches!(
            crate::game_input::toggle_capture_state(),
            crate::game_input::ToggleCapture::Starting
        ) {
            crate::game_input::confirm_toggle_start();
        }
        feedback::play_ptt_start();
        let locale = {
            match ctx.controller.lock() {
                Ok(mut controller) => match controller.finish_ptt_press(&app) {
                    Ok(locale) => Some(locale),
                    Err(error) => {
                        warn!("hotkey ptt state failed: {error}");
                        crate::game_input::reset_toggle_capture();
                        None
                    }
                },
                Err(_) => {
                    crate::game_input::reset_toggle_capture();
                    None
                }
            }
        };
        drop(_ptt_guard);
        log_ptt_trace(&app, "spawn_ptt_press", "worker finished (lock released)");
        if let Some(locale) = locale {
            crate::game_input::overlay::router::on_listening_started(&app, recording_indicator);
            if !suppress_ptt_toasts {
                crate::notify::notify(
                    &app,
                    &i18n::translate(locale, "app.title", &[]),
                    &i18n::translate(locale, "notify.listening", &[]),
                );
            }
            crate::tray::menu::refresh_tray_menu(&app);
        }
    });
}

fn finish_ptt_release_on_controller(
    app: &AppHandle,
    ctx: &Arc<AppContext>,
    speech_queued: bool,
) {
    const ATTEMPTS: u32 = 100;
    let mut controller = None;
    for attempt in 0..ATTEMPTS {
        match ctx.controller.try_lock() {
            Ok(guard) => {
                controller = Some(guard);
                break;
            }
            Err(_) if attempt + 1 == ATTEMPTS => {
                warn!(
                    "ptt release: controller busy after {}ms; skipping finish_ptt_release (pipeline will recover state)",
                    ATTEMPTS * 10
                );
            }
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }

    let Some(mut controller) = controller else {
        return;
    };
    let _ = controller.begin_toggle_stop();
    if let Err(error) = controller.finish_ptt_release(app, speech_queued) {
        warn!("hotkey ptt release state failed: {error}");
    }
    info!(
        ptt_trace = "spawn_ptt_release",
        app_state = ?controller.status().state,
        "controller state after finish_ptt_release"
    );
}

pub(crate) fn spawn_ptt_release(
    app: AppHandle,
    ctx: Arc<AppContext>,
    flush_rx: Option<Receiver<AudioSegment>>,
) {
    log_ptt_trace(&app, "spawn_ptt_release", "worker started");
    std::thread::spawn(move || {
        let locale = app_locale(&app);
        let suppress_ptt_toasts = ctx
            .controller
            .try_lock()
            .ok()
            .is_some_and(|c| c.settings().suppress_ptt_toasts());
        ctx.set_audio_callbacks_enabled(false);

        let flush_rx = {
            let _ptt_guard = ctx.ptt_lock.lock().ok();
            log_ptt_trace(&app, "spawn_ptt_release", "ptt_lock acquired");
            flush_rx.or_else(|| {
                ctx.audio
                    .lock()
                    .ok()
                    .and_then(|mut audio| audio.begin_ptt_release().ok().flatten())
            })
        };

        let speech_queued = match crate::complete_ptt_release(&ctx, &app, flush_rx) {
            Ok(queued) => queued,
            Err(error) => {
                warn!("hotkey ptt release failed: {error}");
                crate::notify::notify(
                    &app,
                    &i18n::translate(locale, "notify.error_title", &[]),
                    &i18n::translate(
                        locale,
                        "notify.capture_stop_failed",
                        &[("error", &error.to_string())],
                    ),
                );
                false
            }
        };

        feedback::play_ptt_stop();

        finish_ptt_release_on_controller(&app, &ctx, speech_queued);
        if !speech_queued {
            crate::game_input::overlay::router::on_listening_stopped(&app);
        }
        if ctx.runtime.pending_count() == 0 && ctx.ptt_postprocess.has_injected_text() {
            crate::app::runtime::schedule_ptt_postprocess_finish(app.clone(), Arc::clone(&ctx));
        }
        ctx.set_audio_callbacks_enabled(true);
        crate::tray::menu::refresh_tray_menu(&app);
        crate::game_input::reset_toggle_capture();

        if !suppress_ptt_toasts {
            if speech_queued {
                crate::notify::notify(
                    &app,
                    &i18n::translate(locale, "app.title", &[]),
                    &i18n::translate(locale, "notify.processing_speech", &[]),
                );
            } else {
                crate::notify::notify(
                    &app,
                    &i18n::translate(locale, "app.title", &[]),
                    &i18n::translate(locale, "notify.no_speech", &[]),
                );
            }
        }
        log_ptt_trace(
            &app,
            "spawn_ptt_release",
            if speech_queued {
                "worker finished (speech queued)"
            } else {
                "worker finished (no speech)"
            },
        );
    });
}

pub fn dispatch_ptt_pressed(app: &AppHandle) {
    log_ptt_trace(app, "dispatch_pressed", "enter");

    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        return;
    };
    let ctx = ctx.inner();

    use crate::game_input::ToggleCapture;

    if matches!(
        crate::game_input::toggle_capture_state(),
        ToggleCapture::Starting | ToggleCapture::Active
    ) {
        let toggle_ptt = ctx.controller.lock().ok().is_some_and(|controller| {
            controller.settings().push_to_talk && press_to_toggle(controller.settings())
        });
        if toggle_ptt {
            log_ptt_trace(app, "dispatch_pressed", "route toggle_stop (capture active)");
            dispatch_ptt_toggle_stop(app);
            return;
        }
        crate::game_input::reset_toggle_capture();
    }

    if crate::game_input::toggle_capture_state() == ToggleCapture::Stopping {
        if ctx.controller.lock().ok().is_some_and(|controller| {
            controller.status().state == AppState::Listening
                || controller.status().state == AppState::Processing
        }) {
            warn!("toggle PTT stop retry while capture still active");
            log_ptt_trace(app, "dispatch_pressed", "retry stop after stuck stopping");
            crate::game_input::reset_toggle_capture();
        } else {
            log_ptt_trace(app, "dispatch_pressed", "ignored while stopping");
            return;
        }
    }

    if ctx.controller.lock().ok().is_some_and(|controller| {
        controller.settings().push_to_talk
            && press_to_toggle(controller.settings())
            && controller.status().state == AppState::Listening
    }) {
        log_ptt_trace(app, "dispatch_pressed", "route toggle_stop (listening)");
        dispatch_ptt_toggle_stop(app);
        return;
    }

    let plan = ctx.controller.lock().ok().and_then(|mut controller| {
        match controller.plan_hotkey_pressed() {
            Ok(HotkeyPlan::None) => Some(HotkeyPlan::None),
            Ok(plan) => {
                info!(ptt_trace = "dispatch_pressed", ?plan, "hotkey plan");
                Some(plan)
            }
            Err(error) => {
                let locale = app_locale(app);
                crate::notify::notify(
                    app,
                    &i18n::translate(locale, "notify.error_title", &[]),
                    &error.to_string(),
                );
                None
            }
        }
    });

    if plan == Some(HotkeyPlan::None) {
        log_ptt_trace(app, "dispatch_pressed", "plan none (hotkey ignored)");
        if let Ok(controller) = ctx.controller.lock() {
            let status = controller.status();
            let locale = controller.settings().ui_locale;
            let message = if status.push_to_talk && status.state != AppState::Ready {
                i18n::translate(
                    locale,
                    "notify.not_ready",
                    &[("state", &state_label(locale, status.state))],
                )
            } else {
                i18n::translate(locale, "notify.hotkey_ignored", &[])
            };
            drop(controller);
            crate::notify::notify(
                app,
                &i18n::translate(locale, "app.title", &[]),
                &message,
            );
        }
        return;
    }

    match plan {
        Some(HotkeyPlan::Toggle(action)) => {
            std::thread::spawn({
                let app = app.clone();
                let ctx = app.try_state::<Arc<AppContext>>().map(|c| c.inner().clone());
                move || {
                    if let Some(ctx) = ctx {
                        if let Err(error) = crate::apply_audio_action(&ctx, &app, action) {
                            warn!("hotkey toggle failed: {error}");
                            let locale = app_locale(&app);
                            crate::notify::notify(
                                &app,
                                &i18n::translate(locale, "notify.error_title", &[]),
                                &error.to_string(),
                            );
                        }
                        crate::tray::menu::refresh_tray_menu(&app);
                    }
                }
            });
        }
        Some(HotkeyPlan::PttPress) => {
            let (toggle_mode, hold_mode) = ctx.controller.lock().ok().map(|c| {
                let settings = c.settings();
                (
                    settings.push_to_talk && press_to_toggle(settings),
                    settings.push_to_talk && !press_to_toggle(settings),
                )
            }).unwrap_or((false, false));
            if (toggle_mode || hold_mode) && !crate::game_input::begin_toggle_start() {
                log_ptt_trace(app, "dispatch_pressed", "begin_toggle_start rejected");
                let _ = ctx.controller.lock().map(|mut c| c.reset_ptt_active());
                return;
            }
            log_ptt_trace(app, "dispatch_pressed", "spawn ptt press");
            let Some(ctx) = app.try_state::<Arc<AppContext>>().map(|c| c.inner().clone()) else {
                if toggle_mode {
                    crate::game_input::reset_toggle_capture();
                }
                return;
            };
            spawn_ptt_press(app.clone(), ctx);
        }
        Some(HotkeyPlan::PttRelease) => {
            // Fallback when the controller is Listening but toggle state was not synced yet.
            if !crate::game_input::begin_toggle_stop() {
                log_ptt_trace(app, "dispatch_pressed", "ptt release plan but begin_stop rejected");
                return;
            }
            log_ptt_trace(app, "dispatch_pressed", "spawn ptt release (plan release)");
            let Some(ctx) = app.try_state::<Arc<AppContext>>().map(|c| c.inner().clone()) else {
                crate::game_input::reset_toggle_capture();
                return;
            };
            spawn_ptt_release(app.clone(), ctx, None);
        }
        _ => {}
    }
}

pub fn dispatch_ptt_toggle_stop(app: &AppHandle) {
    info!(ptt_trace = "dispatch_toggle_stop", "gate received");

    if !crate::game_input::begin_toggle_stop() {
        log_ptt_trace(app, "dispatch_toggle_stop", "begin_toggle_stop rejected");
        return;
    }

    let app = app.clone();
    std::thread::Builder::new()
        .name("veyro-ptt-toggle-stop".into())
        .spawn(move || run_toggle_stop_worker(&app))
        .map_err(|error| {
            warn!("toggle PTT stop worker failed to start: {error}");
            crate::game_input::reset_toggle_capture();
        })
        .ok();
}

fn run_toggle_stop_worker(app: &AppHandle) {
    log_ptt_trace(app, "dispatch_toggle_stop", "worker enter");

    let Some(ctx) = app.try_state::<Arc<AppContext>>().map(|c| c.inner().clone()) else {
        crate::game_input::reset_toggle_capture();
        return;
    };

    info!("toggle PTT stop requested");
    ctx.record_activity(
        Some(app),
        ActivityLevel::Info,
        "activity.ptt.toggle_stop",
        serde_json::json!({}),
    );

    emit_listening_stopped(app);
    crate::game_input::overlay::router::on_listening_stopped(app);

    #[cfg(windows)]
    crate::game_input::hotkey_win::reset_ptt_key_state();

    // Close the audio gate first; do not wait on the controller mutex here (preview/Whisper
    // paths can hold it for hundreds of ms and would freeze ScrollLock stop).
    spawn_ptt_release(app.clone(), ctx, None);
}

pub fn dispatch_ptt_released(app: &AppHandle) {
    log_ptt_trace(app, "dispatch_released", "enter");

    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        return;
    };
    let ctx = ctx.inner();

    let plan = ctx
        .controller
        .lock()
        .ok()
        .and_then(|mut controller| controller.plan_hotkey_released().ok());

    if matches!(plan, Some(HotkeyPlan::PttRelease)) {
        crate::game_input::overlay::router::on_listening_stopped(app);
        let Some(ctx) = app.try_state::<Arc<AppContext>>().map(|c| c.inner().clone()) else {
            return;
        };
        spawn_ptt_release(app.clone(), ctx, None);
    }
}

pub fn handle_shortcut_event(app: &AppHandle, pressed: bool, released: bool) {
    if pressed {
        dispatch_ptt_pressed(app);
    } else if released {
        dispatch_ptt_released(app);
    }
}

