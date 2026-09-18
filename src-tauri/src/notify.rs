use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tracing::{info, warn};

use crate::app::context::AppContext;

pub const APP_USER_MODEL_ID: &str = "com.mkdee.veyro";

/// Whether the OS allows showing toast notifications for this app (global + per-app on Windows).
pub fn system_notifications_available() -> bool {
    #[cfg(windows)]
    {
        return windows_system_notifications_available();
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(windows)]
fn windows_system_notifications_available() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    fn dword_enabled(key: &RegKey, value_name: &str) -> Option<bool> {
        key.get_value::<u32, _>(value_name)
            .ok()
            .map(|value| value != 0)
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let global_enabled = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\PushNotifications")
        .ok()
        .and_then(|key| dword_enabled(&key, "ToastEnabled"))
        .unwrap_or(true);

    if !global_enabled {
        return false;
    }

    let app_key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\{APP_USER_MODEL_ID}"
    );
    hkcu.open_subkey(app_key_path)
        .ok()
        .and_then(|key| dword_enabled(&key, "Enabled"))
        .unwrap_or(true)
}

/// Call once at startup (Windows toast AppUserModelID).
pub fn init_platform() {
    #[cfg(windows)]
    register_windows_app_id();
}

#[cfg(windows)]
fn register_windows_app_id() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let mut wide: Vec<u16> = APP_USER_MODEL_ID.encode_utf16().collect();
    wide.push(0);

    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(wide.as_ptr())) };
    if let Err(error) = result {
        warn!("SetCurrentProcessExplicitAppUserModelID failed: {error}");
    } else {
        info!("Windows toast AppUserModelID registered ({APP_USER_MODEL_ID})");
    }
}

fn user_wants_notifications(app: &AppHandle) -> bool {
    if !system_notifications_available() {
        return false;
    }

    app.try_state::<std::sync::Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.inner()
                .controller
                .try_lock()
                .ok()
                .map(|c| c.settings().show_notifications)
        })
        .unwrap_or(true)
}

pub fn show_system_notification(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| error.to_string())
}

/// User-facing toast when `show_notifications` is enabled in settings.
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if !user_wants_notifications(app) {
        return;
    }

    if let Err(error) = show_system_notification(app, title, body) {
        warn!("system notification failed (title={title:?}): {error}");
    }
}
