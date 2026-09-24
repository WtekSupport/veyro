use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::webview::PageLoadEvent;
use tauri::utils::config::Color;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Size, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window};

use crate::app::events::OVERLAY_LISTENING;
use tracing::{debug, warn};

use crate::app::info;
use crate::app::state::AppState;
use crate::AppContext;
use crate::settings::{UiLocale, UiMode};

pub const SETTINGS_WINDOW_LABEL: &str = "main";
pub const ABOUT_WINDOW_LABEL: &str = "about";
pub const INIT_WINDOW_LABEL: &str = "init";
pub const SKILL_IMPORT_WINDOW_LABEL: &str = "skill-import";
pub const TOOLS_WINDOW_LABEL: &str = "tools";
pub const TOOL_VOICE_FILES_WINDOW_LABEL: &str = "tool-voice-files";
pub const OVERLAY_WINDOW_LABEL: &str = "overlay";

const WINDOW_WIDTH: f64 = 400.0;
const WINDOW_HEIGHT_EXPERT: f64 = 580.0;
const WINDOW_HEIGHT_HOMEMAKER: f64 = 540.0;
const ABOUT_WINDOW_WIDTH: f64 = 360.0;
const ABOUT_WINDOW_HEIGHT: f64 = 560.0;
const INIT_WINDOW_WIDTH: f64 = 320.0;
const INIT_WINDOW_HEIGHT: f64 = 132.0;
const SKILL_IMPORT_WINDOW_WIDTH: f64 = 400.0;
const SKILL_IMPORT_WINDOW_HEIGHT: f64 = 300.0;
const SKILL_IMPORT_WINDOW_HEIGHT_PREVIEW: f64 = 420.0;
const TOOLS_WINDOW_WIDTH: f64 = 400.0;
const TOOLS_WINDOW_HEIGHT: f64 = 360.0;
const TOOL_VOICE_FILES_WINDOW_WIDTH: f64 = 440.0;
const TOOL_VOICE_FILES_WINDOW_HEIGHT: f64 = 520.0;
const OVERLAY_WINDOW_WIDTH: f64 = 140.0;
const OVERLAY_WINDOW_HEIGHT: f64 = 40.0;
const OVERLAY_CORNER_MARGIN: f64 = 16.0;
/// Matches frontend `--bg-deep` (#0d0d0d).
const SETTINGS_WINDOW_BG: Color = Color(13, 13, 13, 255);
const OVERLAY_WINDOW_BG: Color = Color(0, 0, 0, 0);

static OVERLAY_PENDING_LISTENING: AtomicBool = AtomicBool::new(false);
static OVERLAY_PAGE_READY: AtomicBool = AtomicBool::new(false);

fn present_overlay_recording(window: &WebviewWindow) {
    configure_overlay_window(window);
    position_overlay_corner(window);
    show_overlay_without_activation(window);
    sync_overlay_listening(window, true);
}

#[derive(Clone, Serialize)]
struct OverlayListeningPayload {
    active: bool,
}

pub fn sync_overlay_listening(window: &WebviewWindow, active: bool) {
    let _ = window.emit(
        OVERLAY_LISTENING,
        OverlayListeningPayload { active },
    );
    let hidden = !active;
    let script = format!(
        "(function(){{var r=document.querySelector('[data-overlay-rec]');var d=document.querySelector('[data-overlay-listening-dots]');if(r){{r.hidden={hidden};r.setAttribute('aria-hidden','{aria}');}}if(d){{d.hidden={hidden};d.setAttribute('aria-hidden','{aria}');}}}})()",
        hidden = hidden,
        aria = if active { "false" } else { "true" },
    );
    let _ = window.eval(&script);
}

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
    configure_settings_window_chrome(window);
    let _ = window.set_resizable(false);
    let _ = window.set_maximizable(false);
    let _ = window.set_always_on_top(true);
    let height = match ui_mode {
        UiMode::Expert => WINDOW_HEIGHT_EXPERT,
        UiMode::Homemaker => WINDOW_HEIGHT_HOMEMAKER,
    };
    enforce_window_size(window, WINDOW_WIDTH, height);
}

fn configure_settings_window_chrome(window: &WebviewWindow) {
    let _ = window.set_decorations(true);
    let _ = window.set_background_color(Some(SETTINGS_WINDOW_BG));
    #[cfg(windows)]
    apply_windows_titlebar_theme(window);
}

#[cfg(windows)]
fn apply_windows_titlebar_theme(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(hwnd.0 as _);

    // Match frontend `--bg-deep` (#0d0d0d) and `--parchment` (#f0f0f0).
    let dark_mode = 1i32;
    let caption_color: u32 = 0x000d_0d0d;
    let text_color: u32 = 0x00f0_f0f0;

    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_mode as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption_color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            (&text_color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
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
    let _ = window.set_always_on_top(true);
    enforce_window_size(window, ABOUT_WINDOW_WIDTH, ABOUT_WINDOW_HEIGHT);
}

pub fn configure_tools_window(window: &WebviewWindow) {
    let _ = window.set_resizable(true);
    let _ = window.set_maximizable(false);
    let _ = window.set_always_on_top(true);
    enforce_window_size(window, TOOLS_WINDOW_WIDTH, TOOLS_WINDOW_HEIGHT);
}

pub fn configure_voice_files_tool_window(window: &WebviewWindow) {
    configure_settings_window_chrome(window);
    let _ = window.set_resizable(true);
    let _ = window.set_maximizable(false);
    let _ = window.set_always_on_top(true);
    enforce_window_size(
        window,
        TOOL_VOICE_FILES_WINDOW_WIDTH,
        TOOL_VOICE_FILES_WINDOW_HEIGHT,
    );
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

fn ensure_init_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(INIT_WINDOW_LABEL) {
        return Ok(window);
    }

    WebviewWindowBuilder::new(app, INIT_WINDOW_LABEL, WebviewUrl::App("init.html".into()))
        .title("Veyro")
        .inner_size(INIT_WINDOW_WIDTH, INIT_WINDOW_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .visible(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .build()
        .map_err(|error| format!("failed to create init window: {error}"))
}

pub fn show_init_window(app: &AppHandle) {
    if !is_initializing(app) {
        hide_init_window(app);
        return;
    }

    let Ok(window) = ensure_init_window(app) else {
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
    destroy_idle_init_webview(app);
}

pub fn configure_skill_import_window(window: &WebviewWindow) {
    let _ = window.set_resizable(false);
    let _ = window.set_maximizable(false);
    let _ = window.set_always_on_top(true);
    configure_settings_window_chrome(window);
    enforce_window_size(window, SKILL_IMPORT_WINDOW_WIDTH, SKILL_IMPORT_WINDOW_HEIGHT);
}

pub fn resize_skill_import_window(window: &WebviewWindow, preview: bool) {
    configure_skill_import_window(window);
    let height = if preview {
        SKILL_IMPORT_WINDOW_HEIGHT_PREVIEW
    } else {
        SKILL_IMPORT_WINDOW_HEIGHT
    };
    enforce_window_size(window, SKILL_IMPORT_WINDOW_WIDTH, height);
}

fn ensure_skill_import_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(SKILL_IMPORT_WINDOW_LABEL) {
        return Ok(window);
    }

    let window = WebviewWindowBuilder::new(
        app,
        SKILL_IMPORT_WINDOW_LABEL,
        WebviewUrl::App("skill-import.html".into()),
    )
    .title("Veyro")
    .inner_size(SKILL_IMPORT_WINDOW_WIDTH, SKILL_IMPORT_WINDOW_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .center()
    .visible(false)
    .build()
    .map_err(|error| format!("failed to create skill import window: {error}"))?;

    configure_skill_import_window(&window);
    Ok(window)
}

pub fn show_skill_import_window(app: &AppHandle) {
    let Ok(window) = ensure_skill_import_window(app) else {
        debug!("skill import window not available");
        return;
    };

    let locale = settings_ui_locale(app);
    let _ = window.set_title(&crate::i18n::translate(
        locale,
        "skill_import.window_title",
        &[],
    ));
    configure_skill_import_window(&window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_skip_taskbar(false);

    #[cfg(windows)]
    activate_window(&window);

    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
}

pub fn hide_skill_import_window(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(SKILL_IMPORT_WINDOW_LABEL) else {
        return Ok(());
    };
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);
    Ok(())
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
    .always_on_top(true)
    .decorations(true)
    .background_color(SETTINGS_WINDOW_BG)
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

fn destroy_idle_init_webview(app: &AppHandle) {
    if is_initializing(app) {
        return;
    }
    let Some(init) = app.get_webview_window(INIT_WINDOW_LABEL) else {
        return;
    };
    if init.is_visible().unwrap_or(false) {
        return;
    }
    let _ = init.destroy();
    log_webview_released(app, INIT_WINDOW_LABEL);
}

fn destroy_idle_overlay_webview(app: &AppHandle) {
    let Some(overlay) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };
    if overlay.is_visible().unwrap_or(false) {
        return;
    }
    OVERLAY_PAGE_READY.store(false, Ordering::Relaxed);
    let _ = overlay.destroy();
    log_webview_released(app, OVERLAY_WINDOW_LABEL);
}

fn log_webview_released(app: &AppHandle, window_label: &str) {
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.record_activity(
            Some(app),
            crate::app::activity_log::ActivityLevel::Info,
            "activity.memory.webview_destroyed",
            serde_json::json!({ "window": window_label }),
        );
    }
}

/// Destroy tray-idle WebViews after the current window event finishes (never destroy `self` synchronously in handlers).
fn schedule_tray_webview_release(app: &AppHandle) {
    let app = app.clone();
    let runner = app.clone();
    let _ = runner.run_on_main_thread(move || {
        release_main_ui_webviews(&app);
    });
}

fn schedule_about_webview_release(app: &AppHandle) {
    let app = app.clone();
    let runner = app.clone();
    let _ = runner.run_on_main_thread(move || {
        destroy_about_webview(&app);
        log_webview_released(&app, ABOUT_WINDOW_LABEL);
    });
}

/// Tear down settings/about (and hidden init) WebViews while the app stays in the tray.
pub fn release_main_ui_webviews(app: &AppHandle) {
    let had_main = app.get_webview_window(SETTINGS_WINDOW_LABEL).is_some();
    let had_about = app.get_webview_window(ABOUT_WINDOW_LABEL).is_some();

    // About is created with main as parent — tear down child before parent.
    destroy_about_webview(app);
    destroy_settings_webview(app);
    destroy_idle_init_webview(app);
    destroy_idle_overlay_webview(app);

    if had_main {
        log_webview_released(app, SETTINGS_WINDOW_LABEL);
    }
    if had_about {
        log_webview_released(app, ABOUT_WINDOW_LABEL);
    }
}

pub fn maybe_release_webviews_if_minimized(window: &Window) {
    let label = window.label();
    if label != SETTINGS_WINDOW_LABEL && label != ABOUT_WINDOW_LABEL {
        return;
    }
    if !window.is_minimized().unwrap_or(false) {
        return;
    }

    let app = window.app_handle();
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);

    if label == SETTINGS_WINDOW_LABEL {
        schedule_tray_webview_release(&app);
    } else {
        schedule_about_webview_release(&app);
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

    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
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

fn ensure_tools_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(TOOLS_WINDOW_LABEL) {
        return Ok(window);
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        TOOLS_WINDOW_LABEL,
        WebviewUrl::App("tools.html".into()),
    )
    .title("Veyro")
    .inner_size(TOOLS_WINDOW_WIDTH, TOOLS_WINDOW_HEIGHT)
    .resizable(true)
    .maximizable(false)
    .center()
    .visible(false);

    if let Some(parent) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        builder = builder
            .parent(&parent)
            .map_err(|error| format!("failed to attach tools window parent: {error}"))?;
    }

    let window = builder
        .build()
        .map_err(|error| format!("failed to create tools window: {error}"))?;

    configure_tools_window(&window);
    Ok(window)
}

fn ensure_voice_files_tool_window(app: &AppHandle) -> Result<(WebviewWindow, bool), String> {
    if let Some(window) = app.get_webview_window(TOOL_VOICE_FILES_WINDOW_LABEL) {
        return Ok((window, false));
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        TOOL_VOICE_FILES_WINDOW_LABEL,
        WebviewUrl::App("tool-voice-files.html".into()),
    )
    .title("Veyro")
    .inner_size(TOOL_VOICE_FILES_WINDOW_WIDTH, TOOL_VOICE_FILES_WINDOW_HEIGHT)
    .decorations(true)
    .resizable(true)
    .maximizable(false)
    .closable(true)
    .center()
    .visible(false)
    .background_color(SETTINGS_WINDOW_BG)
    .drag_and_drop(true);

    if let Some(parent) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        builder = builder
            .parent(&parent)
            .map_err(|error| format!("failed to attach voice files tool window parent: {error}"))?;
    }

    let window = builder
        .build()
        .map_err(|error| format!("failed to create voice files tool window: {error}"))?;

    configure_voice_files_tool_window(&window);
    Ok((window, true))
}

pub fn show_tools_window(app: &AppHandle) -> Result<(), String> {
    let window = ensure_tools_window(app).inspect_err(|error| {
        warn!("tools window creation failed: {error}");
    })?;

    let _ = window.set_title(&crate::i18n::translate(
        settings_ui_locale(app),
        "tools.window_title",
        &[],
    ));
    configure_tools_window(&window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_skip_taskbar(false);

    #[cfg(windows)]
    activate_window(&window);

    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    Ok(())
}

pub fn show_voice_files_tool_window(app: &AppHandle) -> Result<(), String> {
    let (window, created) = ensure_voice_files_tool_window(app).inspect_err(|error| {
        warn!("voice files tool window creation failed: {error}");
    })?;

    let _ = window.set_title(&crate::i18n::translate(
        settings_ui_locale(app),
        "tools.voice_files.window_title",
        &[],
    ));
    configure_voice_files_tool_window(&window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_skip_taskbar(false);

    #[cfg(windows)]
    activate_window(&window);

    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();

    if !created {
        crate::app::events::emit_voice_files_window_ready(app);
    }

    Ok(())
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
    Ok(())
}

/// Hide the settings window, destroy its WebView, and keep it out of the taskbar.
pub fn hide_settings_window(window: &Window) {
    let app = window.app_handle();
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);

    if window.label() == SETTINGS_WINDOW_LABEL {
        schedule_tray_webview_release(&app);
    } else if window.label() == ABOUT_WINDOW_LABEL {
        schedule_about_webview_release(&app);
    }
}

pub fn configure_overlay_window(window: &WebviewWindow) {
    let _ = window.set_decorations(false);
    let _ = window.set_background_color(Some(OVERLAY_WINDOW_BG));
    #[cfg(not(windows))]
    let _ = window.set_always_on_top(true);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_shadow(false);
    let _ = window.set_size(Size::Logical(LogicalSize::new(
        OVERLAY_WINDOW_WIDTH,
        OVERLAY_WINDOW_HEIGHT,
    )));
    position_overlay_corner(window);
    #[cfg(windows)]
    sync_overlay_win32_chrome(window);
}

/// Hidden WebView for REC indicator (avoids creating the window during fullscreen capture).
pub fn prewarm_recording_overlay(app: &AppHandle) {
    let _ = ensure_overlay_window(app);
}

fn ensure_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        return Ok(window);
    }

    let window = WebviewWindowBuilder::new(
        app,
        OVERLAY_WINDOW_LABEL,
        WebviewUrl::App("overlay.html".into()),
    )
    .title("")
    .inner_size(OVERLAY_WINDOW_WIDTH, OVERLAY_WINDOW_HEIGHT)
    .decorations(false)
    .transparent(true)
    .background_color(OVERLAY_WINDOW_BG)
    .shadow(false)
    .always_on_top(true)
    .visible(false)
    .skip_taskbar(true)
    .on_page_load(|window, payload| {
        if payload.event() != PageLoadEvent::Finished {
            return;
        }
        OVERLAY_PAGE_READY.store(true, Ordering::Relaxed);
        if !OVERLAY_PENDING_LISTENING.load(Ordering::Relaxed) {
            return;
        }
        let window = window.clone();
        let app = window.app_handle().clone();
        let _ = app.run_on_main_thread(move || {
            present_overlay_recording(&window);
        });
    })
    .build()
    .map_err(|error| format!("failed to create overlay window: {error}"))?;

    configure_overlay_window(&window);
    Ok(window)
}

pub fn show_overlay_recording(app: &AppHandle) -> Result<(), String> {
    OVERLAY_PENDING_LISTENING.store(true, Ordering::Relaxed);
    let window = ensure_overlay_window(app)?;

    if OVERLAY_PAGE_READY.load(Ordering::Relaxed) {
        present_overlay_recording(&window);
    }
    Ok(())
}

pub fn hide_overlay(app: &AppHandle) {
    OVERLAY_PENDING_LISTENING.store(false, Ordering::Relaxed);
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };
    hide_overlay_without_activation(&window);
    sync_overlay_listening(&window, false);
}

#[cfg(windows)]
fn show_overlay_without_activation(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        SW_SHOWNOACTIVATE,
    };

    sync_overlay_win32_chrome(window);
    let Ok(raw) = window.hwnd() else {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        return;
    };
    let hwnd = HWND(raw.0 as _);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    sync_overlay_win32_chrome(window);
}

#[cfg(not(windows))]
fn show_overlay_without_activation(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.set_always_on_top(true);
}

#[cfg(windows)]
fn hide_overlay_without_activation(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    if let Ok(raw) = window.hwnd() {
        unsafe {
            let _ = ShowWindow(HWND(raw.0 as _), SW_HIDE);
        }
    } else {
        let _ = window.hide();
    }
}

#[cfg(not(windows))]
fn hide_overlay_without_activation(window: &WebviewWindow) {
    let _ = window.hide();
}

fn position_overlay_corner(window: &WebviewWindow) {
    let monitor = overlay_target_monitor(window);
    if let Some(monitor) = monitor {
        let size = monitor.size();
        let origin = monitor.position();
        let scale = monitor.scale_factor();
        let width = OVERLAY_WINDOW_WIDTH * scale;
        let margin = OVERLAY_CORNER_MARGIN * scale;
        let x = origin.x as f64 + size.width as f64 - width - margin;
        let y = origin.y as f64 + margin;
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: x.round() as i32,
            y: y.round() as i32,
        }));
        #[cfg(windows)]
        sync_overlay_win32_chrome(window);
    }
}

fn overlay_target_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
    if let Some(monitor) = crate::injection::focus_target::monitor_for_injection_target(window) {
        return Some(monitor);
    }
    if let Some(monitor) = foreground_monitor(window) {
        return Some(monitor);
    }
    window.current_monitor().ok().flatten()
}

#[cfg(windows)]
fn foreground_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };

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

    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    let x = ((rect.left + rect.right) / 2) as f64;
    let y = ((rect.top + rect.bottom) / 2) as f64;
    window
        .monitor_from_point(x, y)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
}

#[cfg(not(windows))]
fn foreground_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
    window.current_monitor().ok().flatten()
}

#[cfg(not(windows))]
fn sync_overlay_win32_chrome(_window: &WebviewWindow) {}

#[cfg(windows)]
fn windows_build_number() -> u32 {
    use windows::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};

    unsafe {
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        if GetVersionExW(&mut info).is_ok() {
            info.dwBuildNumber
        } else {
            0
        }
    }
}

/// Frameless transparent REC surface: strip caption/sysmenu, DWM glass, re-apply after move/show.
#[cfg(windows)]
fn sync_overlay_win32_chrome(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
        DWMWA_SYSTEMBACKDROP_TYPE,
    };
    use windows::Win32::UI::Controls::MARGINS;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOPMOST,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_CAPTION, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
    };

    /// Win11+ draws an opaque Mica/acrylic plate unless backdrop is disabled.
    const DWMSBT_NONE: i32 = 3;
    /// Windows 11 first public builds (DWM backdrop APIs are not safe on Win10).
    const WIN11_MIN_BUILD: u32 = 22000;

    let Ok(raw) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(raw.0 as _);
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let chrome = WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU;
        let style = style & !(chrome.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize,
        );

        let nc_disabled = DWMNCRP_DISABLED.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            (&nc_disabled as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );

        if windows_build_number() >= WIN11_MIN_BUILD {
            let backdrop_none = DWMSBT_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                (&backdrop_none as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            );
        }
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

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
