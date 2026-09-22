use tauri::AppHandle;

/// Show top-right REC overlay (call only when `recording_indicator` is enabled).
pub fn show_recording_overlay(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let _ = crate::window::show_overlay_recording(&app);
    });
}

pub fn hide_recording_overlay(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        crate::window::hide_overlay(&app);
    });
}

pub fn on_listening_started(app: &AppHandle, recording_indicator: bool) {
    if recording_indicator {
        show_recording_overlay(app);
    }
}

pub fn on_listening_stopped(app: &AppHandle) {
    hide_recording_overlay(app);
}
