use std::sync::Mutex;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::info;

use crate::error::AppError;
#[cfg(not(windows))]
use crate::game_input::HotkeyInstallConfig;
use crate::hotkey::manager::handle_shortcut_event;
use crate::hotkey::normalize::{normalize_hotkey, parse_hotkey};

static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);

#[cfg(not(windows))]
pub fn install_low_level(app: &AppHandle, config: &HotkeyInstallConfig) -> Result<(), AppError> {
    use crate::hotkey::low_level;

    if let Ok(mut guard) = APP_HANDLE.lock() {
        *guard = Some(app.clone());
    }
    low_level::register(app, &config.hotkey)
}

#[cfg(not(windows))]
pub fn update_low_level(config: &HotkeyInstallConfig) -> Result<(), AppError> {
    use crate::hotkey::low_level;

    let app = APP_HANDLE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .ok_or_else(|| AppError::Hotkey("low-level hotkey app handle missing".to_string()))?;
    low_level::unregister();
    low_level::register(&app, &config.hotkey)
}

#[cfg(not(windows))]
pub fn uninstall_low_level() {
    use crate::hotkey::low_level;

    low_level::unregister();
    if let Ok(mut guard) = APP_HANDLE.lock() {
        *guard = None;
    }
}

pub fn install_global_shortcut(app: &AppHandle, hotkey: &str) -> Result<(), AppError> {
    if let Ok(mut guard) = APP_HANDLE.lock() {
        *guard = Some(app.clone());
    }
    unregister_all(app);

    let normalized = normalize_hotkey(hotkey);
    let shortcut = parse_hotkey(hotkey)?;
    let shortcut_manager = app.global_shortcut();

    shortcut_manager
        .on_shortcut(shortcut, |app, shortcut, event| {
            info!("hotkey event: {} state={:?}", shortcut, event.state);
            handle_shortcut_event(
                app,
                event.state == ShortcutState::Pressed,
                event.state == ShortcutState::Released,
            );
        })
        .map_err(|error| AppError::Hotkey(error.to_string()))?;

    info!("registered global hotkey: {normalized}");
    Ok(())
}

pub fn uninstall_global_shortcut() {
    if let Ok(guard) = APP_HANDLE.lock() {
        if let Some(app) = guard.as_ref() {
            let _ = app.global_shortcut().unregister_all();
        }
    }
}

fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}
