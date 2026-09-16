use std::sync::Arc;

use tauri::{AppHandle, LogicalSize, Manager, Size, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window};
use tracing::{debug, warn};

use crate::app::info;
use crate::app::state::AppState;
use crate::AppContext;
use crate::settings::{UiLocale, UiMode};

pub const SETTINGS_WINDOW_LABEL: &str = "main";
pub const ABOUT_WINDOW_LABEL: &str = "about";
pub const INIT_WINDOW_LABEL: &str = "init";
pub const OVERLAY_WINDOW_LABEL: &str = "overlay";

const WINDOW_WIDTH: f64 = 400.0;
const WINDOW_HEIGHT_EXPERT: f64 = 580.0;
const WINDOW_HEIGHT_HOMEMAKER: f64 = 540.0;
const ABOUT_WINDOW_WIDTH: f64 = 360.0;
const ABOUT_WINDOW_HEIGHT: f64 = 560.0;
const INIT_WINDOW_WIDTH: f64 = 320.0;
const INIT_WINDOW_HEIGHT: f64 = 132.0;
const OVERLAY_WINDOW_WIDTH: f64 = 280.0;
const OVERLAY_WINDOW_HEIGHT: f64 = 56.0;
const OVERLAY_CORNER_MARGIN: f64 = 16.0;

fn settings_ui_locale(app: &AppHandle) -> UiLocale {
    app.try_state::<Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.controller
                .lock()
                .ok()
                .map(|controller| controller.settings().ui_locale)
        })
        .unwrap_or(UiLocale::Ru)
}

fn settings_ui_mode(app: &AppHandle) -> UiMode {
    app.try_state::<Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.controller
                .lock()
                .ok()
                .map(|controller| controller.settings().ui_mode)
        })
        .unwrap_or(UiMode::Homemaker)
}

/// Fixed-size settings window: no maximize button and no resize/maximize by drag or double-click.
pub fn configure_window(app: &AppHandle, window: &WebviewWindow) {
    configure_window_for_ui_mode(window, settings_ui_mode(app));
}

pub fn configure_window_for_ui_mode(window: &WebviewWindow, ui_mode: UiMode) {
    let _ = window.set_resizable(false);
    let _ = window.set_maximizable(false);
    let height = match ui_mode {
        UiMode::Expert => WINDOW_HEIGHT_EXPERT,
        UiMode::Homemaker => WINDOW_HEIGHT_HOMEMAKER,
    };
    enforce_window_size(window, WINDOW_WIDTH, height);
}

pub fn configure_main_window_for_ui_mode(app: &AppHandle, ui_mode: UiMode) {
    let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) else {
        debug!("settings window not found for ui mode resize");
        return;
    };
    configure_window_for_ui_mode(&window, ui_mode);
}

pub fn configure_about_window(window: &WebviewWindow) {
    let _ = window.set_resizable(false);
    let _ = window.set_maximizable(false);
    enforce_window_size(window, ABOUT_WINDOW_WIDTH, ABOUT_WINDOW_HEIGHT);
}

fn enforce_window_size(window: &WebviewWindow, width: f64, height: f64) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }

    let scale = window.scale_factor().unwrap_or(1.0);
    if let Ok(size) = window.outer_size() {
        let current_width = size.width as f64 / scale;
        let current_height = size.height as f64 / scale;
        if (current_width - width).abs() > 1.0 || (current_height - height).abs() > 1.0 {
            let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
        }
    }
}

pub fn is_initializing(app: &AppHandle) -> bool {
    app.try_state::<Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.controller
                .lock()
                .ok()
                .map(|controller| controller.status().state == AppState::Initializing)
        })
        .unwrap_or(false)
}

pub fn configure_init_window(window: &WebviewWindow) {
    let _ = window.set_resizable(false);
    let _ = window.set_maximizable(false);
    let _ = window.set_minimizable(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_skip_taskbar(true);
    enforce_window_size(window, INIT_WINDOW_WIDTH, INIT_WINDOW_HEIGHT);
}

pub fn show_init_window(app: &AppHandle) {
    if !is_initializing(app) {
        hide_init_window(app);
        return;
    }

    let Some(window) = app.get_webview_window(INIT_WINDOW_LABEL) else {
        debug!("init window not found");
        return;
    };

    configure_init_window(&window);
    let _ = window.set_title(&crate::i18n::translate(
        settings_ui_locale(app),
        "app.title",
        &[],
    ));
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide_init_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(INIT_WINDOW_LABEL) else {
        return;
    };
    let _ = window.hide();
}

fn ensure_settings_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        return Ok(window);
    }

    let ui_mode = settings_ui_mode(app);
    let height = match ui_mode {
        UiMode::Expert => WINDOW_HEIGHT_EXPERT,
        UiMode::Homemaker => WINDOW_HEIGHT_HOMEMAKER,
    };

    let window = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("Veyro")
    .inner_size(WINDOW_WIDTH, height)
    .resizable(false)
    .maximizable(false)
    .center()
    .visible(false)
    .skip_taskbar(true)
    .build()
    .map_err(|error| format!("failed to create settings window: {error}"))?;

    configure_window_for_ui_mode(&window, ui_mode);
    Ok(window)
}

pub fn destroy_settings_webview(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.destroy();
    }
}

pub fn destroy_about_webview(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(ABOUT_WINDOW_LABEL) {
        let _ = window.destroy();
    }
}

/// Show the settings window and bring it to the foreground.
pub fn show_settings_window(app: &AppHandle) {
    if is_initializing(app) {
        show_init_window(app);
        return;
    }

    hide_init_window(app);

    let Ok(window) = ensure_settings_window(app) else {
        debug!("settings window could not be created");
        return;
    };

    // Avoid restoring focus to the last injection target when opening settings.
    crate::injection::focus_target::clear_injection_target();

    configure_window(app, &window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_skip_taskbar(false);

    #[cfg(windows)]
    activate_window(&window);

    // Brief always-on-top helps after text injection steals foreground on Windows.
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = window.set_always_on_top(false);
}

fn ensure_about_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(ABOUT_WINDOW_LABEL) {
        return Ok(window);
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        ABOUT_WINDOW_LABEL,
        WebviewUrl::App("about.html".into()),
    )
    .title("About Veyro")
    .inner_size(ABOUT_WINDOW_WIDTH, ABOUT_WINDOW_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .center()
    .visible(false);

    if let Some(parent) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        builder = builder
            .parent(&parent)
            .map_err(|error| format!("failed to attach about window parent: {error}"))?;
    }

    let window = builder
        .build()
        .map_err(|error| format!("failed to create about window: {error}"))?;

    configure_about_window(&window);
    Ok(window)
}

/// Run a UI-thread-only task and await its result without blocking the event loop.
pub async fn await_on_main_thread<R, F>(app: &AppHandle, f: F) -> Result<R, String>
where
    R: Send + 'static,
    F: FnOnce() -> R + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|error| format!("failed to schedule main-thread task: {error}"))?;
    rx.await
        .map_err(|_| "main-thread task interrupted".to_string())
}

pub fn show_about_window(app: &AppHandle) -> Result<(), String> {
    let window = ensure_about_window(app).inspect_err(|error| {
        warn!("about window creation failed: {error}");
    })?;

    let _ = window.set_title(&info::about_window_title(settings_ui_locale(app)));
    configure_about_window(&window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_skip_taskbar(false);

    #[cfg(windows)]
    activate_window(&window);

    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = window.set_always_on_top(false);
    Ok(())
}

/// Hide the settings window, destroy its WebView, and keep it out of the taskbar.
pub fn hide_settings_window(window: &Window) {
    let app = window.app_handle();
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);

    if window.label() == SETTINGS_WINDOW_LABEL {
        destroy_settings_webview(&app);
        if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
            ctx.record_activity(
                Some(&app),
                crate::app::activity_log::ActivityLevel::Info,
                "activity.memory.webview_destroyed",
                serde_json::json!({ "window": SETTINGS_WINDOW_LABEL }),
            );
        }
    } else if window.label() == ABOUT_WINDOW_LABEL {
        destroy_about_webview(&app);
        if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
            ctx.record_activity(
                Some(&app),
                crate::app::activity_log::ActivityLevel::Info,
                "activity.memory.webview_destroyed",
                serde_json::json!({ "window": ABOUT_WINDOW_LABEL }),
            );
        }
    }
}

pub fn configure_overlay_window(window: &WebviewWindow) {
    let _ = window.set_decorations(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_shadow(false);
    let _ = window.set_size(Size::Logical(LogicalSize::new(
        OVERLAY_WINDOW_WIDTH,
        OVERLAY_WINDOW_HEIGHT,
    )));
    position_overlay_corner(window);
    configure_overlay_extended_style(window);
}

pub fn show_overlay_recording(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return Err("overlay window not found".to_string());
    };

    configure_overlay_window(&window);
    position_overlay_corner(&window);
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    Ok(())
}

pub fn hide_overlay(app: &AppHandle) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };
    let _ = window.hide();
}

fn position_overlay_corner(window: &WebviewWindow) {
    let monitor = overlay_target_monitor(window);
    if let Some(monitor) = monitor {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let width = OVERLAY_WINDOW_WIDTH * scale;
        let height = OVERLAY_WINDOW_HEIGHT * scale;
        let margin = OVERLAY_CORNER_MARGIN * scale;
        let x = size.width as f64 - width - margin;
        let y = size.height as f64 - height - margin;
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: x.round() as i32,
            y: y.round() as i32,
        }));
    }
}

fn overlay_target_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
    if let Some(monitor) = foreground_monitor(window) {
        return Some(monitor);
    }
    window.current_monitor().ok().flatten()
}

#[cfg(windows)]
fn foreground_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }

    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }

    if pid == std::process::id() {
        return window.current_monitor().ok().flatten();
    }

    window.primary_monitor().ok().flatten()
}

#[cfg(not(windows))]
fn foreground_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
    window.current_monitor().ok().flatten()
}

#[cfg(windows)]
fn configure_overlay_extended_style(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0 as _);
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                style | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize,
            );
        }
    }
}

#[cfg(not(windows))]
fn configure_overlay_extended_style(_window: &WebviewWindow) {}

#[cfg(windows)]
fn activate_window(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, SetForegroundWindow, ShowWindow, SW_SHOW,
    };

    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0 as _);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn activate_window(_window: &WebviewWindow) {}
