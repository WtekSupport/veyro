pub mod audio_gate;
pub mod combo;
pub mod hotkey_fallback;
#[cfg(windows)]
pub mod binding_state_win;
#[cfg(windows)]
pub mod elevation_win;
#[cfg(windows)]
pub mod hotkey_win;
pub mod overlay;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::error::AppError;
use crate::hotkey::normalize::normalize_hotkey;
#[cfg(not(windows))]
use crate::hotkey::binding::KeyBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PttSignal {
    Pressed,
    Released,
    /// Toggle PTT stop (latched keys such as ScrollLock in press-to-toggle mode).
    ToggleStop,
}

#[derive(Debug, Clone)]
pub struct HotkeyInstallConfig {
    pub hotkey: String,
    /// On non-Windows, selects low-level hook vs global shortcut. Windows always uses the hook.
    #[cfg(not(windows))]
    pub game_mode: bool,
    pub block_system: bool,
    pub ptt_hold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyBackend {
    None,
    LlHook,
    GlobalShortcut,
}

static ACTIVE_BACKEND: Mutex<HotkeyBackend> = Mutex::new(HotkeyBackend::None);
static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Lifecycle of a toggle-PTT take (press starts, next press stops).
///
/// This is the single source of truth for toggle mode: it is readable from the keyboard hook
/// thread without locking the controller, and `Starting`/`Stopping` cover the windows where the
/// audio stream is opening or the segment is being flushed. Extra key edges arriving in those
/// windows are swallowed instead of opening a second capture on top of the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ToggleCapture {
    Idle = 0,
    Starting = 1,
    Active = 2,
    Stopping = 3,
}

impl ToggleCapture {
    fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Starting,
            2 => Self::Active,
            3 => Self::Stopping,
            _ => Self::Idle,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Active => "active",
            Self::Stopping => "stopping",
        }
    }
}

static TOGGLE_CAPTURE: AtomicU8 = AtomicU8::new(ToggleCapture::Idle as u8);

pub fn toggle_capture_state() -> ToggleCapture {
    ToggleCapture::from_raw(TOGGLE_CAPTURE.load(Ordering::Acquire))
}

/// True from the moment a start press is accepted until the take is fully released.
pub fn toggle_capture_active() -> bool {
    matches!(
        toggle_capture_state(),
        ToggleCapture::Starting | ToggleCapture::Active
    )
}

/// Claim the take for a starting press. Fails when a take is already open or in transition.
pub fn begin_toggle_start() -> bool {
    let from = toggle_capture_state();
    let ok = swap_toggle_state(ToggleCapture::Idle, ToggleCapture::Starting);
    if ok {
        info!(
            ptt_toggle = true,
            from = from.label(),
            to = ToggleCapture::Starting.label(),
            "toggle capture claimed for start"
        );
    } else {
        warn!(
            ptt_toggle = true,
            current = from.label(),
            "toggle capture begin_start rejected (not idle)"
        );
    }
    ok
}

/// Promote a claimed take to `Active` once the capture stream is open. A stop that already won
/// the race keeps its `Stopping` claim.
pub fn confirm_toggle_start() {
    let from = toggle_capture_state();
    let ok = swap_toggle_state(ToggleCapture::Starting, ToggleCapture::Active);
    if ok {
        info!(
            ptt_toggle = true,
            from = from.label(),
            to = ToggleCapture::Active.label(),
            "toggle capture stream ready"
        );
    } else {
        warn!(
            ptt_toggle = true,
            current = from.label(),
            "toggle capture confirm_start skipped"
        );
    }
}

/// Claim the stop edge for the current take. Fails when a stop is already running.
pub fn begin_toggle_stop() -> bool {
    let from = toggle_capture_state();
    let ok = TOGGLE_CAPTURE
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |raw| {
            (ToggleCapture::from_raw(raw) != ToggleCapture::Stopping)
                .then_some(ToggleCapture::Stopping as u8)
        })
        .is_ok();
    if ok {
        info!(
            ptt_toggle = true,
            from = from.label(),
            to = ToggleCapture::Stopping.label(),
            "toggle capture claimed for stop"
        );
    } else {
        warn!(
            ptt_toggle = true,
            current = from.label(),
            "toggle capture begin_stop rejected (already stopping)"
        );
    }
    ok
}

/// Release the take so the next press can open a new one.
pub fn reset_toggle_capture() {
    let from = toggle_capture_state();
    if from != ToggleCapture::Idle {
        info!(
            ptt_toggle = true,
            from = from.label(),
            to = ToggleCapture::Idle.label(),
            "toggle capture reset"
        );
    }
    TOGGLE_CAPTURE.store(ToggleCapture::Idle as u8, Ordering::Release);
}

fn swap_toggle_state(from: ToggleCapture, to: ToggleCapture) -> bool {
    TOGGLE_CAPTURE
        .compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

pub fn active_backend() -> HotkeyBackend {
    *ACTIVE_BACKEND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn hook_active() -> bool {
    HOOK_ACTIVE.load(Ordering::Relaxed)
}

pub fn backend_label() -> &'static str {
    match active_backend() {
        HotkeyBackend::None => "none",
        HotkeyBackend::LlHook => "ll_hook",
        HotkeyBackend::GlobalShortcut => "global_shortcut",
    }
}

pub fn register(app: &AppHandle, config: HotkeyInstallConfig) -> Result<HotkeyBackend, AppError> {
    audio_gate::ensure_running(app.clone());
    unregister_internal();

    if should_use_ll_hook(&config) {
        #[cfg(windows)]
        {
            match hotkey_win::install(app, &config) {
                Ok(()) => {
                    set_backend(HotkeyBackend::LlHook, true);
                    log_hook_installed(app, &config.hotkey);
                    return Ok(HotkeyBackend::LlHook);
                }
                Err(error) => {
                    log_hook_failed(app, &error);
                }
            }
        }

        #[cfg(not(windows))]
        {
            match hotkey_fallback::install_low_level(app, &config) {
                Ok(()) => {
                    set_backend(HotkeyBackend::LlHook, true);
                    log_hook_installed(app, &config.hotkey);
                    return Ok(HotkeyBackend::LlHook);
                }
                Err(error) => {
                    log_hook_failed(app, &error);
                }
            }
        }
    }

    hotkey_fallback::install_global_shortcut(app, &config.hotkey)?;
    set_backend(HotkeyBackend::GlobalShortcut, false);
    log_global_fallback(app, &config.hotkey);
    Ok(HotkeyBackend::GlobalShortcut)
}

pub fn update_config(app: &AppHandle, config: HotkeyInstallConfig) -> Result<(), AppError> {
    let want_hook = should_use_ll_hook(&config);
    let have_hook = active_backend() == HotkeyBackend::LlHook;

    if want_hook != have_hook {
        return register(app, config).map(|_| ());
    }

    if have_hook {
        #[cfg(windows)]
        hotkey_win::update_config(&config)?;
        #[cfg(not(windows))]
        hotkey_fallback::update_low_level(&config)?;
        info!(
            "game hotkey spec updated: {}",
            normalize_hotkey(&config.hotkey)
        );
        Ok(())
    } else {
        register(app, config).map(|_| ())
    }
}

pub(crate) fn unregister() {
    unregister_internal();
    set_backend(HotkeyBackend::None, false);
}

fn unregister_internal() {
    #[cfg(windows)]
    hotkey_win::uninstall();
    #[cfg(not(windows))]
    hotkey_fallback::uninstall_low_level();
    hotkey_fallback::uninstall_global_shortcut();
}

fn should_use_ll_hook(config: &HotkeyInstallConfig) -> bool {
    // Global shortcuts (RegisterHotKey) are missed by many hosts — TC command line, Sublime,
    // games. On Windows always prefer the low-level hook; `hotkey_game_mode` only toggles UI
    // options such as blocking keys from reaching the host app.
    #[cfg(windows)]
    {
        let _ = config;
        return crate::hotkey::game_mode_supported();
    }

    #[cfg(not(windows))]
    {
        crate::hotkey::game_mode_supported()
            && (config.game_mode || KeyBinding::needs_edge_hook(&config.hotkey))
    }
}

fn set_backend(backend: HotkeyBackend, hook: bool) {
    if let Ok(mut guard) = ACTIVE_BACKEND.lock() {
        *guard = backend;
    }
    HOOK_ACTIVE.store(hook, Ordering::Relaxed);
}

fn log_hook_installed(app: &AppHandle, hotkey: &str) {
    record_hotkey_activity(
        app,
        ActivityLevel::Info,
        "activity.hotkey.ll_hook_installed",
        hotkey,
    );
}

fn log_hook_failed(app: &AppHandle, error: &AppError) {
    record_activity(
        app,
        ActivityLevel::Warn,
        "activity.hotkey.game_hook_unavailable",
        serde_json::json!({ "error": error.to_string() }),
    );
}

fn log_global_fallback(app: &AppHandle, hotkey: &str) {
    record_hotkey_activity(
        app,
        ActivityLevel::Info,
        "activity.hotkey.global_shortcut_active",
        hotkey,
    );
}

fn record_activity(app: &AppHandle, level: ActivityLevel, key: &str, args: serde_json::Value) {
    let Some(ctx) = app.try_state::<std::sync::Arc<AppContext>>() else {
        return;
    };
    ctx.record_activity(Some(app), level, key, args);
}

fn record_hotkey_activity(app: &AppHandle, level: ActivityLevel, key: &str, hotkey: &str) {
    record_activity(
        app,
        level,
        key,
        serde_json::json!({ "hotkey": normalize_hotkey(hotkey) }),
    );
}

#[cfg(windows)]
pub fn is_process_elevated() -> bool {
    elevation_win::is_process_elevated()
}

#[cfg(not(windows))]
pub fn is_process_elevated() -> bool {
    false
}

#[cfg(windows)]
pub fn hotkeys_blocked_by_foreground_elevation() -> bool {
    elevation_win::hotkeys_blocked_by_foreground_elevation()
}

#[cfg(not(windows))]
pub fn hotkeys_blocked_by_foreground_elevation() -> bool {
    false
}

/// Log elevation status at startup and optionally notify when not running as admin.
#[cfg(windows)]
pub fn report_startup_elevation(app: &AppHandle) {
    use crate::i18n;
    use crate::notify;
    use crate::settings::UiLocale;

    let locale = app
        .try_state::<std::sync::Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.inner()
                .controller
                .try_lock()
                .ok()
                .map(|controller| controller.settings().ui_locale)
        })
        .unwrap_or(UiLocale::En);

    if is_process_elevated() {
        record_activity(
            app,
            ActivityLevel::Info,
            "activity.elevation.admin_ok",
            serde_json::json!({}),
        );
        return;
    }

    record_activity(
        app,
        ActivityLevel::Warn,
        "activity.elevation.not_elevated",
        serde_json::json!({}),
    );
    notify::notify(
        app,
        &i18n::translate(locale, "app.title", &[]),
        &i18n::translate(locale, "notify.elevation_not_elevated", &[]),
    );
}

#[cfg(not(windows))]
pub fn report_startup_elevation(_app: &AppHandle) {}

/// Log when foreground elevation blocks PTT (poll thread); `notify` is false to avoid spam.
#[cfg(windows)]
pub fn report_elevation_mismatch(app: &AppHandle, notify: bool) {
    use crate::game_input::elevation_win::{check_elevation_mismatch, ElevationMismatch};
    use crate::i18n;
    use crate::notify;
    use crate::settings::UiLocale;

    match check_elevation_mismatch() {
        ElevationMismatch::Ok => {}
        ElevationMismatch::VeyroNotElevated => {
            record_activity(
                app,
                ActivityLevel::Error,
                "activity.elevation.not_elevated",
                serde_json::json!({}),
            );
            if notify {
                let locale = app
                    .try_state::<std::sync::Arc<AppContext>>()
                    .and_then(|ctx| {
                        ctx.inner()
                            .controller
                            .try_lock()
                            .ok()
                            .map(|controller| controller.settings().ui_locale)
                    })
                    .unwrap_or(UiLocale::En);
                notify::notify(
                    app,
                    &i18n::translate(locale, "app.title", &[]),
                    &i18n::translate(locale, "notify.elevation_not_elevated", &[]),
                );
            }
        }
        ElevationMismatch::ForegroundHigherIntegrity => {
            record_activity(
                app,
                ActivityLevel::Warn,
                "activity.elevation.foreground_higher",
                serde_json::json!({}),
            );
            if notify {
                let locale = app
                    .try_state::<std::sync::Arc<AppContext>>()
                    .and_then(|ctx| {
                        ctx.inner()
                            .controller
                            .try_lock()
                            .ok()
                            .map(|controller| controller.settings().ui_locale)
                    })
                    .unwrap_or(UiLocale::En);
                notify::notify(
                    app,
                    &i18n::translate(locale, "app.title", &[]),
                    &i18n::translate(locale, "notify.elevation_foreground_higher", &[]),
                );
            }
        }
    }
}

#[cfg(not(windows))]
pub fn report_elevation_mismatch(_app: &AppHandle, _notify: bool) {}

#[cfg(test)]
mod toggle_capture_tests {
    use super::*;

    /// `TOGGLE_CAPTURE` is process-global, so the lifecycle cases share one serialized test.
    #[test]
    fn toggle_capture_lifecycle() {
        reset_toggle_capture();
        assert_eq!(toggle_capture_state(), ToggleCapture::Idle);
        assert!(!toggle_capture_active());

        assert!(begin_toggle_start());
        assert_eq!(toggle_capture_state(), ToggleCapture::Starting);
        // A take counts as open while the stream is still opening.
        assert!(toggle_capture_active());
        // A second press cannot claim a new take.
        assert!(!begin_toggle_start());

        confirm_toggle_start();
        assert_eq!(toggle_capture_state(), ToggleCapture::Active);
        assert!(!begin_toggle_start());

        assert!(begin_toggle_stop());
        assert_eq!(toggle_capture_state(), ToggleCapture::Stopping);
        // Only one stop may run at a time, and no press may start during it.
        assert!(!begin_toggle_stop());
        assert!(!begin_toggle_start());

        reset_toggle_capture();
        assert!(begin_toggle_start());

        // A stop that wins the race against a slow start keeps the take closing.
        assert!(begin_toggle_stop());
        confirm_toggle_start();
        assert_eq!(toggle_capture_state(), ToggleCapture::Stopping);

        reset_toggle_capture();
    }
}

