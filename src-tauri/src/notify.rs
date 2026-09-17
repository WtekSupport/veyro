use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let show = app
        .try_state::<std::sync::Arc<crate::app::context::AppContext>>()
        .and_then(|ctx| {
            ctx.inner()
                .controller
                .try_lock()
                .ok()
                .map(|c| c.settings().show_notifications)
        })
        .unwrap_or(true);

    if !show {
        return;
    }

    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}
