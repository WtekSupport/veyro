use std::sync::Arc;
use std::time::Duration;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::app::context::AppContext;
use crate::app::state::{AppState, StatusSnapshot};
use crate::i18n::{self, state_label};
use crate::settings::UiLocale;
use crate::tools::voice_watch;
use crate::tray::state::{TrayIconSet, TrayState};
use crate::window::{hide_init_window, show_init_window, show_settings_window, show_voice_files_tool_window};

const TRAY_ID_SETTINGS: &str = "settings";
const TRAY_ID_QUIT: &str = "quit";
const TRAY_ID_VOICE_WATCH: &str = "voice_watch";
const TRAY_ID_RECENT_PREFIX: &str = "voice_recent:";
const TRAY_ID_RECENT_EMPTY: &str = "voice_recent_empty";

#[derive(Clone)]
pub struct TrayVisualSnapshot {
    pub status: StatusSnapshot,
    pub locale: UiLocale,
    pub voice_watch_enabled: bool,
    pub voice_watch_cloud: bool,
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, TRAY_ID_SETTINGS, "Settings", false, None::<&str>)?;
    let voice_watch_item = CheckMenuItem::with_id(
        app,
        TRAY_ID_VOICE_WATCH,
        "Transcribe voice messages",
        true,
        false,
        None::<&str>,
    )?;
    let recent_empty = MenuItem::with_id(
        app,
        TRAY_ID_RECENT_EMPTY,
        "No recent transcripts",
        false,
        None::<&str>,
    )?;
    let r0 = MenuItem::with_id(app, format!("{TRAY_ID_RECENT_PREFIX}0"), "—", false, None::<&str>)?;
    let r1 = MenuItem::with_id(app, format!("{TRAY_ID_RECENT_PREFIX}1"), "—", false, None::<&str>)?;
    let r2 = MenuItem::with_id(app, format!("{TRAY_ID_RECENT_PREFIX}2"), "—", false, None::<&str>)?;
    let r3 = MenuItem::with_id(app, format!("{TRAY_ID_RECENT_PREFIX}3"), "—", false, None::<&str>)?;
    let r4 = MenuItem::with_id(app, format!("{TRAY_ID_RECENT_PREFIX}4"), "—", false, None::<&str>)?;
    let recent_sub = Submenu::with_id_and_items(
        app,
        "recent_transcripts",
        "Recent transcripts",
        true,
        &[&recent_empty, &r0, &r1, &r2, &r3, &r4],
    )?;
    let recent_items = vec![r0, r1, r2, r3, r4];
    let quit = MenuItem::with_id(app, TRAY_ID_QUIT, "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &voice_watch_item,
            &recent_sub,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray_state = TrayState::new(
        app,
        settings,
        quit,
        voice_watch_item,
        recent_sub,
        recent_items,
        recent_empty,
    );
    let initial_icon = tray_state.icons.initializing.clone();

    app.manage(tray_state);

    TrayIconBuilder::with_id("main")
        .icon(initial_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Veyro — Starting")
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            match id {
                TRAY_ID_SETTINGS => {
                    show_settings_window(app);
                }
                TRAY_ID_VOICE_WATCH => {
                    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
                        let enabled = ctx
                            .inner()
                            .controller
                            .lock()
                            .ok()
                            .map(|c| !c.settings().voice_watch.enabled)
                            .unwrap_or(true);
                        let _ = voice_watch::voice_watch_set_enabled(
                            app,
                            Arc::clone(ctx.inner()),
                            enabled,
                        );
                        refresh_tray_menu(app);
                    }
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
                other if other.starts_with(TRAY_ID_RECENT_PREFIX) => {
                    let slot: usize = other[TRAY_ID_RECENT_PREFIX.len()..]
                        .parse()
                        .unwrap_or(usize::MAX);
                    if let Some(tray_state) = app.try_state::<TrayState>() {
                        let ids = tray_state.recent_ids.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(Some(hist_id)) = ids.get(slot) {
                            let _ = show_voice_files_tool_window(app);
                            let _ = app.emit("app://voice-history-open", hist_id.clone());
                        }
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Enter { .. } => show_init_window(app),
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => show_settings_window(app),
                _ => {}
            }
        })
        .build(app)?;

    refresh_tray_menu(app);
    Ok(())
}

use tauri::Emitter;

/// Push a tray refresh using a snapshot already read under the controller lock.
pub fn schedule_tray_visual(app: &AppHandle, snapshot: TrayVisualSnapshot) {
    let handle = app.clone();
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
    let settings = controller.settings();
    let cloud = voice_watch::is_cloud_provider_active(settings)
        && settings.voice_watch.enabled;
    Some(TrayVisualSnapshot {
        status: controller.status(),
        locale: settings.ui_locale,
        voice_watch_enabled: settings.voice_watch.enabled,
        voice_watch_cloud: cloud,
    })
}

fn apply_tray_visual(app: &AppHandle, snapshot: TrayVisualSnapshot) {
    let Some(tray_state) = app.try_state::<TrayState>() else {
        return;
    };

    let TrayVisualSnapshot {
        status,
        locale,
        voice_watch_enabled,
        voice_watch_cloud,
    } = snapshot;

    let _ = tray_state
        .settings_item()
        .set_text(i18n::translate(locale, "tray.settings", &[]));
    let _ = tray_state
        .quit_item()
        .set_text(i18n::translate(locale, "tray.quit", &[]));

    let watch_label = if voice_watch_cloud {
        i18n::translate(locale, "tray.voice_watch_cloud", &[])
    } else {
        i18n::translate(locale, "tray.voice_watch", &[])
    };
    let _ = tray_state.voice_watch_item().set_text(watch_label);
    let _ = tray_state.voice_watch_item().set_checked(voice_watch_enabled);

    let _ = tray_state
        .recent_submenu()
        .set_text(&i18n::translate(locale, "tray.recent_transcripts", &[]));

    let recent = voice_watch::list_history()
        .into_iter()
        .filter(|e| matches!(e.status, voice_watch::HistoryStatus::Done) && !e.text.is_empty())
        .take(5)
        .collect::<Vec<_>>();

    {
        let mut ids = tray_state
            .recent_ids
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for slot in 0..5 {
            ids[slot] = recent.get(slot).map(|e| e.id.clone());
            if let Some(item) = tray_state.recent_items().get(slot) {
                if let Some(entry) = recent.get(slot) {
                    let preview: String = entry.text.chars().take(40).collect();
                    let label = format!("{} — {preview}", entry.file_name);
                    let _ = item.set_text(label);
                    let _ = item.set_enabled(true);
                } else {
                    let _ = item.set_text("—");
                    let _ = item.set_enabled(false);
                }
            }
        }
    }
    let _ = tray_state.recent_empty().set_text(i18n::translate(
        locale,
        "tray.recent_empty",
        &[],
    ));
    let _ = tray_state
        .recent_empty()
        .set_enabled(false);

    let initializing = status.state == AppState::Initializing;
    let (icon, tooltip) = tray_visual(&status, locale, &tray_state.icons);
    let _ = tray_state.settings_item().set_enabled(!initializing);
    if !initializing {
        hide_init_window(app);
    }

    if let Some(tray) = app.tray_by_id("main") {
        if !crate::tray::blink::is_purple_blink_active() {
            let _ = tray.set_icon(Some(icon));
        }
        let _ = tray.set_tooltip(Some(tooltip.as_str()));
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
