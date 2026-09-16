use std::sync::Arc;
use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::app::context::AppContext;
use crate::app::state::{AppState, StatusSnapshot};
use crate::i18n::{self, state_label};
use crate::settings::UiLocale;
use crate::tray::state::{TrayIconSet, TrayState};
use crate::window::{hide_init_window, show_init_window, show_settings_window};

const TRAY_ID_SETTINGS: &str = "settings";
const TRAY_ID_QUIT: &str = "quit";

#[derive(Clone)]
pub struct TrayVisualSnapshot {
    pub status: StatusSnapshot,
    pub locale: UiLocale,
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, TRAY_ID_SETTINGS, "Settings", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_ID_QUIT, "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&settings, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let tray_state = TrayState::new(app, settings, quit);
    let initial_icon = tray_state.icons.initializing.clone();

    app.manage(tray_state);

    TrayIconBuilder::with_id("main")
        .icon(initial_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Veyro — Starting")
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                TRAY_ID_SETTINGS => {
                    show_settings_window(app);
                }
                TRAY_ID_QUIT => {
                    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
                        ctx.inner().cancel_pending();
                        if let Ok(mut audio) = ctx.inner().audio.lock() {
                            if let Ok(vad_join) = audio.stop() {
                                drop(audio);
                                if let Some(handle) = vad_join {
                                    let _ = handle.join();
                                }
                            }
                        }
                    }
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Enter { .. } => show_init_window(&app),
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => show_settings_window(&app),
                _ => {}
            }
        })
        .build(app)?;

    refresh_tray_menu(app);
    Ok(())
}

/// Push a tray refresh using a snapshot already read under the controller lock.
pub fn schedule_tray_visual(app: &AppHandle, snapshot: TrayVisualSnapshot) {
    let handle = app.clone();
    // Never block the caller on the UI thread: PTT workers may hold `controller` while
    // transitioning state, and a synchronous `run_on_main_thread` deadlocks with the main
    // thread when it tries to lock the controller for settings/status IPC.
    std::thread::spawn(move || {
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            apply_tray_visual(&app, snapshot);
        });
    });
}

/// Schedule tray icon refresh on the UI thread (required for reliable updates on Windows).
pub fn refresh_tray_menu(app: &AppHandle) {
    if let Some(snapshot) = read_tray_snapshot(app) {
        schedule_tray_visual(app, snapshot);
        return;
    }

    // `transition()` may call refresh while holding `controller.lock()` on this thread.
    let app = app.clone();
    std::thread::spawn(move || {
        for delay_ms in [16_u64, 32, 64, 128] {
            std::thread::sleep(Duration::from_millis(delay_ms));
            if let Some(snapshot) = read_tray_snapshot(&app) {
                schedule_tray_visual(&app, snapshot);
                return;
            }
        }
    });
}

fn read_tray_snapshot(app: &AppHandle) -> Option<TrayVisualSnapshot> {
    let ctx = app.try_state::<Arc<AppContext>>()?;
    let controller = ctx.inner().controller.try_lock().ok()?;
    Some(TrayVisualSnapshot {
        status: controller.status(),
        locale: controller.settings().ui_locale,
    })
}

fn apply_tray_visual(app: &AppHandle, snapshot: TrayVisualSnapshot) {
    let Some(tray_state) = app.try_state::<TrayState>() else {
        return;
    };

    let TrayVisualSnapshot { status, locale } = snapshot;

    let _ = tray_state
        .settings_item()
        .set_text(i18n::translate(locale, "tray.settings", &[]));
    let _ = tray_state
        .quit_item()
        .set_text(i18n::translate(locale, "tray.quit", &[]));

    let initializing = status.state == AppState::Initializing;
    let (icon, tooltip) = tray_visual(&status, locale, &tray_state.icons);
    let _ = tray_state.settings_item().set_enabled(!initializing);
    if !initializing {
        hide_init_window(app);
    }

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}

fn tray_visual(
    status: &StatusSnapshot,
    locale: UiLocale,
    icons: &TrayIconSet,
) -> (tauri::image::Image<'static>, String) {
    if status.state == AppState::Listening {
        return (
            icons.listening.clone(),
            i18n::translate(locale, "tray.tooltip.listening", &[]),
        );
    }

    if matches!(
        status.state,
        AppState::Processing | AppState::Transcribing | AppState::Injecting
    ) {
        let state = state_label(locale, status.state);
        return (
            icons.processing.clone(),
            i18n::translate(locale, "tray.tooltip.state", &[("state", &state)]),
        );
    }

    if matches!(
        status.state,
        AppState::Error
            | AppState::PermissionRequired
            | AppState::MicrophoneUnavailable
            | AppState::NetworkUnavailable
    ) {
        let state = state_label(locale, status.state);
        return (
            icons.error.clone(),
            i18n::translate(locale, "tray.tooltip.state", &[("state", &state)]),
        );
    }

    if status.state == AppState::Initializing {
        return (
            icons.initializing.clone(),
            i18n::translate(locale, "tray.tooltip.starting", &[]),
        );
    }

    if status.state == AppState::Disabled {
        let state = state_label(locale, status.state);
        return (
            icons.idle.clone(),
            i18n::translate(locale, "tray.tooltip.state", &[("state", &state)]),
        );
    }

    if status.push_to_talk {
        (
            icons.ready.clone(),
            i18n::translate(
                locale,
                "tray.tooltip.ready_hold",
                &[("hotkey", &status.hotkey)],
            ),
        )
    } else {
        (
            icons.ready.clone(),
            i18n::translate(locale, "tray.tooltip.ready", &[]),
        )
    }
}
