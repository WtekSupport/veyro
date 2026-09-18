use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app::context::AppContext;
use crate::i18n;

use super::menu;
use super::state::TrayState;

const BLINK_INTERVAL_MS: u64 = 450;

static PURPLE_BLINK_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn start_purple_blink(app: &AppHandle) {
    if PURPLE_BLINK_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let app = app.clone();
    thread::spawn(move || {
        let mut bright = true;
        while PURPLE_BLINK_ACTIVE.load(Ordering::SeqCst) {
            let icon = app
                .try_state::<TrayState>()
                .map(|state| {
                    if bright {
                        state.icons.ai_deleting_bright.clone()
                    } else {
                        state.icons.ai_deleting_dim.clone()
                    }
                });

            if let Some(icon) = icon {
                let app_for_main = app.clone();
                let tooltip = app
                    .try_state::<std::sync::Arc<AppContext>>()
                    .and_then(|ctx| {
                        ctx.controller.lock().ok().map(|c| {
                            i18n::translate(c.settings().ui_locale, "tray.ai_deleting", &[])
                        })
                    })
                    .unwrap_or_else(|| "Veyro — AI replacing text".to_string());
                let _ = app.run_on_main_thread(move || {
                    if let Some(tray) = app_for_main.tray_by_id("main") {
                        let _ = tray.set_icon(Some(icon));
                        let _ = tray.set_tooltip(Some(&tooltip));
                    }
                });
            }

            bright = !bright;
            thread::sleep(Duration::from_millis(BLINK_INTERVAL_MS));
        }

        menu::refresh_tray_menu(&app);
    });
}

pub fn stop_purple_blink(app: &AppHandle) {
    if PURPLE_BLINK_ACTIVE.swap(false, Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(BLINK_INTERVAL_MS + 20));
        menu::refresh_tray_menu(app);
    }
}

pub fn is_purple_blink_active() -> bool {
    PURPLE_BLINK_ACTIVE.load(Ordering::SeqCst)
}
