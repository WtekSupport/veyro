use tauri::AppHandle;
use tracing::debug;

use crate::window;

pub fn show_recording(app: &AppHandle) {
    if let Err(error) = window::show_overlay_recording(app) {
        debug!("overlay show failed: {error}");
    }
}

pub fn hide(app: &AppHandle) {
    window::hide_overlay(app);
}
