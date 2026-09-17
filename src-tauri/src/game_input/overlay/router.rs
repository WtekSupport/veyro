use tauri::{AppHandle, Manager};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::game_input::overlay::{detect, topmost};

pub fn on_ptt_pressed(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || show_overlay_if_needed(&app));
}

pub fn on_ptt_released(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || topmost::hide(&app));
}

fn show_overlay_if_needed(app: &AppHandle) {
    match detect::current_display_mode() {
        detect::DisplayMode::Windowed => topmost::show_recording(app),
        detect::DisplayMode::ExclusiveFullscreen => {
            log_overlay_suppressed(app);
            if has_secondary_monitor() {
                topmost::show_recording(app);
            }
        }
    }
}

fn log_overlay_suppressed(app: &AppHandle) {
    let Some(ctx) = app.try_state::<std::sync::Arc<AppContext>>() else {
        return;
    };
    ctx.record_activity(
        Some(app),
        ActivityLevel::Info,
        "activity.overlay.suppressed_exclusive",
        serde_json::json!({}),
    );
}

#[cfg(windows)]
fn has_secondary_monitor() -> bool {
    use std::sync::OnceLock;

    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(enumerate_secondary_monitor)
}

#[cfg(windows)]
fn enumerate_secondary_monitor() -> bool {
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::EnumDisplayMonitors;

    struct MonitorCount {
        count: u32,
    }

    unsafe extern "system" fn count_monitors(
        _monitor: windows::Win32::Graphics::Gdi::HMONITOR,
        _hdc: windows::Win32::Graphics::Gdi::HDC,
        _rect: *mut RECT,
        state: LPARAM,
    ) -> BOOL {
        let state = &mut *(state.0 as *mut MonitorCount);
        state.count += 1;
        BOOL(1)
    }

    let mut state = MonitorCount { count: 0 };
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(count_monitors),
            LPARAM(&mut state as *mut MonitorCount as isize),
        );
    }
    state.count > 1
}

#[cfg(not(windows))]
fn has_secondary_monitor() -> bool {
    false
}
