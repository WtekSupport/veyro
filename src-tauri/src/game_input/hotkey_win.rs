use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use tauri::AppHandle;
use tracing::{error, info, warn};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::error::AppError;
use crate::game_input::audio_gate;
use crate::injection::windows_keyboard;
use crate::game_input::binding_state_win;
use crate::game_input::combo::{self, KeyEvent};
use crate::game_input::HotkeyInstallConfig;
use crate::hotkey::binding::{KeyBinding, KeyEventTarget, ModifierState};
use crate::hotkey::normalize::normalize_hotkey;

const LLKHF_REPEAT: u32 = 0x4000_0000;
const LLKHF_INJECTED: u32 = 0x0000_0010;
const POLL_INTERVAL: Duration = Duration::from_millis(8);
const ELEVATION_WARN_INTERVAL: Duration = Duration::from_secs(2);

struct RuntimeConfig {
    binding: KeyBinding,
    block_system: bool,
    ptt_hold: bool,
}

struct HookComboState {
    mods: ModifierState,
    combo_active: bool,
}

struct PollHandle {
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

struct HookHandle {
    hook_thread: JoinHandle<()>,
    worker_thread: JoinHandle<()>,
}

const HOOK_EVENT_CHANNEL_CAPACITY: usize = 256;
type EventSender = Sender<KeyEvent>;
static EVENT_TX: Mutex<Option<EventSender>> = Mutex::new(None);

static RUNTIME_CONFIG: RwLock<Option<Arc<RwLock<RuntimeConfig>>>> = RwLock::new(None);
static HOOK_THREAD_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static HOOK_COMBO_STATE: Mutex<HookComboState> = Mutex::new(HookComboState {
    mods: ModifierState {
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    },
    combo_active: false,
});
static PTT_HELD: AtomicBool = AtomicBool::new(false);
static ACTIVE_POLL: Mutex<Option<PollHandle>> = Mutex::new(None);
static ACTIVE_HOOK: Mutex<Option<HookHandle>> = Mutex::new(None);
pub fn install(app: &AppHandle, config: &HotkeyInstallConfig) -> Result<(), AppError> {
    uninstall();

    let binding = KeyBinding::parse(&config.hotkey)?;
    let runtime = Arc::new(RwLock::new(RuntimeConfig {
        binding,
        block_system: config.block_system,
        ptt_hold: config.ptt_hold,
    }));
    if let Ok(mut guard) = RUNTIME_CONFIG.write() {
        *guard = Some(runtime);
    }
    reset_hook_combo_state();

    start_poll_thread(app)?;

    start_hook()?;

    info!(
        ptt_trace = "hook_install",
        hotkey = %normalize_hotkey(&config.hotkey),
        hook_ptt_hold = config.ptt_hold,
        block_system = config.block_system,
        "installed game hotkey backend"
    );

    Ok(())
}

pub fn update_config(config: &HotkeyInstallConfig) -> Result<(), AppError> {
    let binding = KeyBinding::parse(&config.hotkey)?;

    if let Ok(guard) = RUNTIME_CONFIG.read() {
        if let Some(runtime) = guard.as_ref() {
            if let Ok(mut inner) = runtime.write() {
                inner.binding = binding;
                inner.block_system = config.block_system;
                inner.ptt_hold = config.ptt_hold;
            }
            reset_hook_combo_state();
            info!(
                "game hotkey spec updated: {}",
                normalize_hotkey(&config.hotkey)
            );
        }
    } else {
        return Err(AppError::Hotkey(
            "game keyboard backend is not running".to_string(),
        ));
    }

    Ok(())
}

pub fn uninstall() {
    stop_hook();
    stop_poll_thread();
    reset_hook_combo_state();

    if let Ok(mut guard) = RUNTIME_CONFIG.write() {
        *guard = None;
    }
    info!("uninstalled game keyboard backend");
}

/// Forget the physical key state so the next key-down produces a fresh edge. Deliberately leaves
/// the toggle take lifecycle alone — the take is owned by `game_input::ToggleCapture`.
pub fn reset_ptt_key_state() {
    reset_hook_combo_state();
}

fn reset_hook_combo_state() {
    PTT_HELD.store(false, Ordering::Release);
    if let Ok(mut state) = HOOK_COMBO_STATE.lock() {
        state.mods = binding_state_win::read_modifier_state();
        state.combo_active = false;
    }
}

fn dispatch_latched_toggle_keydown() {
    use crate::game_input::ToggleCapture;

    let toggle = crate::game_input::toggle_capture_state();
    let signal = match toggle {
        ToggleCapture::Idle => {
            emit_ptt_pressed();
            "pressed"
        }
        ToggleCapture::Starting | ToggleCapture::Active => {
            emit_ptt_toggle_stop();
            "toggle_stop"
        }
        ToggleCapture::Stopping => "ignored_stopping",
    };
    info!(
        ptt_trace = "hook_latched_keydown",
        toggle = toggle.label(),
        hook_ptt_hold = runtime_ptt_hold(),
        signal,
    );
}

fn dispatch_latched_keydown() {
    if runtime_ptt_hold() {
        emit_ptt_pressed();
        info!(
            ptt_trace = "hook_latched_keydown",
            hook_ptt_hold = true,
            signal = "pressed_hold",
        );
        return;
    }
    dispatch_latched_toggle_keydown();
}

/// Latched keys (ScrollLock etc.) bypass the bounded hook worker queue so edges are never dropped
/// when injected keys or other events flood `EVENT_TX`.
fn try_dispatch_latched_key_direct(vk: u32, pressed: bool) -> bool {
    let Some((binding, _block_system)) = current_runtime_config() else {
        return false;
    };
    if !binding.uses_latched_or_modifier_only() {
        return false;
    }

    let Some(event) = binding_state_win::hook_key_event(vk, pressed) else {
        return false;
    };

    let mods = binding_state_win::read_modifier_state();
    let trigger_match =
        binding.event_matches_trigger(&event.target) && binding.mods_match(&mods);

    if pressed && trigger_match {
        info!(
            ptt_trace = "hook_latched_direct",
            vk,
            pressed,
            hook_ptt_hold = runtime_ptt_hold(),
        );
        dispatch_latched_keydown();
        if !runtime_ptt_hold() {
            reset_hook_combo_state();
        }
        return true;
    }
    if !pressed && trigger_match {
        if runtime_ptt_hold() {
            info!(ptt_trace = "hook_latched_keyup", vk, signal = "released_hold");
            emit_ptt_released();
        } else {
            info!(ptt_trace = "hook_latched_keyup", vk, "cleared PTT_HELD");
            PTT_HELD.store(false, Ordering::Release);
        }
        return true;
    }

    false
}

fn emit_ptt_toggle_stop() {
    audio_gate::try_send_signal(crate::game_input::PttSignal::ToggleStop);
}

/// Hold-mode PTT can miss key-up after Alt+Tab; resync before accepting a new edge.
fn reconcile_ptt_held_with_physical_binding() {
    if !runtime_ptt_hold() {
        return;
    }
    let Some(binding) = current_binding() else {
        return;
    };
    if binding_state_win::binding_pressed(&binding) {
        return;
    }
    if PTT_HELD
        .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        audio_gate::try_send_signal(crate::game_input::PttSignal::Released);
    }
}

fn emit_ptt_pressed() {
    reconcile_ptt_held_with_physical_binding();
    if PTT_HELD.load(Ordering::Acquire) {
        // Toggle keys (ScrollLock/CapsLock) can miss key-up; still deliver the press edge.
        if !runtime_ptt_hold() {
            audio_gate::try_send_signal(crate::game_input::PttSignal::Pressed);
        }
        return;
    }

    if PTT_HELD
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        audio_gate::try_send_signal(crate::game_input::PttSignal::Pressed);
    }
}

fn emit_ptt_released() {
    // Always clear the held flag so the next key-down is seen as a new edge by the poll thread,
    // but only hold mode ends its take on key-up. In toggle mode the take runs until the second
    // press, so the release must not reach the controller at all.
    if PTT_HELD
        .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
        && runtime_ptt_hold()
    {
        audio_gate::try_send_signal(crate::game_input::PttSignal::Released);
    }
}

fn start_poll_thread(app: &AppHandle) -> Result<(), AppError> {
    stop_poll_thread();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let app = app.clone();
    let thread = thread::Builder::new()
        .name("veyro-game-poll".into())
        .spawn(move || poll_thread_main(stop_flag, app))
        .map_err(|error| AppError::Hotkey(format!("game poll thread failed: {error}")))?;

    if let Ok(mut guard) = ACTIVE_POLL.lock() {
        *guard = Some(PollHandle { stop, thread });
    }
    Ok(())
}

fn stop_poll_thread() {
    let handle = ACTIVE_POLL.lock().ok().and_then(|mut guard| guard.take());
    let Some(handle) = handle else {
        return;
    };

    handle.stop.store(true, Ordering::Release);
    if handle.thread.join().is_err() {
        warn!("game poll thread panicked");
    }
}

fn spawn_hook_worker(rx: Receiver<KeyEvent>) -> Result<JoinHandle<()>, AppError> {
    thread::Builder::new()
        .name("veyro-game-hook-worker".into())
        .spawn(move || {
            while let Ok(event) = rx.recv() {
                handle_hook_key_event(event);
            }
        })
        .map_err(|error| AppError::Hotkey(format!("game hook worker thread failed: {error}")))
}

fn start_hook() -> Result<(), AppError> {
    if ACTIVE_HOOK
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|_| ()))
        .is_some()
    {
        return Ok(());
    }

    let (tx, rx) = bounded(HOOK_EVENT_CHANNEL_CAPACITY);
    if let Ok(mut guard) = EVENT_TX.lock() {
        *guard = Some(tx);
    }

    let worker_thread = spawn_hook_worker(rx)?;

    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let hook_thread = thread::Builder::new()
        .name("veyro-game-hook".into())
        .spawn(move || hook_thread_main(ready_tx))
        .map_err(|error| AppError::Hotkey(format!("game hook thread failed: {error}")))?;

    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            if let Ok(mut guard) = EVENT_TX.lock() {
                *guard = None;
            }
            if worker_thread.join().is_err() {
                warn!("game hook worker thread panicked during startup");
            }
            return Err(error);
        }
        Err(_) => {
            if let Ok(mut guard) = EVENT_TX.lock() {
                *guard = None;
            }
            if worker_thread.join().is_err() {
                warn!("game hook worker thread panicked during startup");
            }
            return Err(AppError::Hotkey(
                "low-level keyboard hook timed out".to_string(),
            ));
        }
    }

    if let Ok(mut guard) = ACTIVE_HOOK.lock() {
        *guard = Some(HookHandle {
            hook_thread,
            worker_thread,
        });
    }
    Ok(())
}

fn stop_hook() {
    let handle = ACTIVE_HOOK.lock().ok().and_then(|mut guard| guard.take());
    let Some(handle) = handle else {
        return;
    };

    if let Ok(mut guard) = EVENT_TX.lock() {
        *guard = None;
    }

    if handle.worker_thread.join().is_err() {
        warn!("game hook worker thread panicked");
    }

    let thread_id = HOOK_THREAD_ID.load(Ordering::Acquire);
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }

    if handle.hook_thread.join().is_err() {
        warn!("game hook thread panicked");
    }
    HOOK_THREAD_ID.store(0, Ordering::Release);
}

fn current_binding() -> Option<KeyBinding> {
    current_runtime_config().map(|(binding, _)| binding)
}

fn current_runtime_config() -> Option<(KeyBinding, bool)> {
    let guard = RUNTIME_CONFIG.read().ok()?;
    let runtime = guard.as_ref()?.clone();
    let config = runtime.read().ok()?;
    Some((config.binding.clone(), config.block_system))
}

fn runtime_ptt_hold() -> bool {
    RUNTIME_CONFIG
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().cloned())
        .and_then(|runtime| runtime.read().ok().map(|cfg| cfg.ptt_hold))
        .unwrap_or(true)
}

pub fn set_ptt_hold(hold: bool) {
    if let Ok(guard) = RUNTIME_CONFIG.read() {
        if let Some(runtime) = guard.as_ref() {
            if let Ok(mut inner) = runtime.write() {
                inner.ptt_hold = hold;
            }
        }
    }
}

fn poll_thread_main(stop: Arc<AtomicBool>, app: AppHandle) {
    let mut elevation_warned = false;
    let mut elevation_checks = 0u32;

    while !stop.load(Ordering::Acquire) {
        elevation_checks = elevation_checks.wrapping_add(1);
        let elevation_every =
            (ELEVATION_WARN_INTERVAL.as_millis() / POLL_INTERVAL.as_millis().max(1)).max(1) as u32;
        if !elevation_warned && elevation_checks.is_multiple_of(elevation_every) {
            let mismatch = crate::game_input::elevation_win::check_elevation_mismatch();
            if mismatch != crate::game_input::elevation_win::ElevationMismatch::Ok {
                match mismatch {
                    crate::game_input::elevation_win::ElevationMismatch::VeyroNotElevated => {
                        error!(
                            "Veyro is not running as administrator; PTT and injection into elevated apps are unavailable"
                        );
                    }
                    crate::game_input::elevation_win::ElevationMismatch::ForegroundHigherIntegrity => {
                        warn!(
                            "foreground app has higher integrity than Veyro; PTT hotkeys may not reach Veyro"
                        );
                    }
                    _ => {}
                }
                crate::game_input::report_elevation_mismatch(&app, false);
                elevation_warned = true;
            }
        }
        let Some(binding) = current_binding() else {
            thread::sleep(POLL_INTERVAL);
            continue;
        };

        // Toggle keys and modifier-only bindings rely on hook edge events; polling their
        // latched GetAsyncKeyState causes missed presses and spurious releases.
        if binding.uses_latched_or_modifier_only() {
            thread::sleep(POLL_INTERVAL);
            continue;
        }

        // Hold-to-talk must use hook down/up edges only. While the hook is running, polling
        // GetAsyncKeyState in parallel produces brief "not pressed" glitches during a physical
        // hold and retriggers press/release in a loop.
        if runtime_ptt_hold() {
            thread::sleep(POLL_INTERVAL);
            continue;
        }

        let pressed = binding_state_win::binding_pressed(&binding);
        let held = PTT_HELD.load(Ordering::Acquire);

        if pressed && !held {
            emit_ptt_pressed();
        } else if !pressed && held {
            emit_ptt_released();
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn hook_thread_main(ready: std::sync::mpsc::Sender<Result<(), AppError>>) {
    unsafe {
        HOOK_THREAD_ID.store(GetCurrentThreadId(), Ordering::Release);

        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), HINSTANCE::default(), 0)
        {
            Ok(hook) => hook,
            Err(error) => {
                let message = format!("SetWindowsHookExW failed: {error}");
                warn!("{message}");
                let _ = ready.send(Err(AppError::Hotkey(message)));
                return;
            }
        };

        if ready.send(Ok(())).is_err() {
            let _ = UnhookWindowsHookEx(hook);
            return;
        }

        let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
        HOOK_THREAD_ID.store(0, Ordering::Release);
    }
}

fn reconcile_combo_state(binding: &KeyBinding, state: &mut HookComboState) {
    if !state.combo_active || PTT_HELD.load(Ordering::Acquire) {
        return;
    }

    // Toggle keys keep GetAsyncKeyState high after release; rely on hook edges instead.
    if binding.uses_latched_or_modifier_only() {
        return;
    }

    if !binding_state_win::binding_pressed(binding) {
        state.combo_active = false;
    }
}

fn hook_modifier_state(state: &mut HookComboState, event: KeyEvent) -> ModifierState {
    let mut mods = state.mods;
    match event.target {
        KeyEventTarget::Modifier(kind) => mods.set(kind, event.pressed),
        // Host apps (Sublime, TC) may swallow modifier key-down events; resync on trigger press.
        KeyEventTarget::Key(_) if event.pressed => {
            mods = binding_state_win::read_modifier_state();
        }
        _ => {}
    }
    mods
}

fn handle_hook_key_event(event: KeyEvent) {
    let Some((binding, _block_system)) = current_runtime_config() else {
        return;
    };

    reconcile_ptt_held_with_physical_binding();

    let Ok(mut state) = HOOK_COMBO_STATE.lock() else {
        return;
    };

    reconcile_combo_state(&binding, &mut state);

    let mut mods = hook_modifier_state(&mut state, event);

    if binding.uses_latched_or_modifier_only() {
        let trigger_match =
            binding.event_matches_trigger(&event.target) && binding.mods_match(&mods);
        let latched_trigger_down = event.pressed && trigger_match;
        let latched_trigger_up = !event.pressed && trigger_match;

        if runtime_ptt_hold() {
            if latched_trigger_down {
                emit_ptt_pressed();
            } else if latched_trigger_up {
                emit_ptt_released();
            }
            state.mods = mods;
            return;
        }

        if latched_trigger_down || !event.pressed {
            if latched_trigger_down {
                dispatch_latched_toggle_keydown();
            }
            state.mods = mods;
            state.combo_active = false;
            PTT_HELD.store(false, Ordering::Release);
            return;
        }
    }

    let mut combo_active = state.combo_active;
    let allow_latched_repeat = false;
    if let Some(signal) = combo::process_combo_event(
        &binding,
        &mut mods,
        &mut combo_active,
        event,
        allow_latched_repeat,
    ) {
        match signal {
            crate::game_input::PttSignal::Pressed => emit_ptt_pressed(),
            crate::game_input::PttSignal::Released => emit_ptt_released(),
            crate::game_input::PttSignal::ToggleStop => emit_ptt_toggle_stop(),
        }
    }
    state.mods = mods;
    state.combo_active = combo_active;
}

unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code != HC_ACTION as i32 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let msg = wparam.0 as u32;
    let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let released = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    if !pressed && !released {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
    if pressed && (kb.flags.0 & LLKHF_REPEAT) != 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    // Ignore our own injected keystrokes; otherwise they flood
    // the hook queue and real hotkey edges (ScrollLock stop) get dropped on try_send.
    if (kb.flags.0 & LLKHF_INJECTED) != 0
        || windows_keyboard::is_tagged_injection(kb.dwExtraInfo)
    {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let direct_toggle = try_dispatch_latched_key_direct(kb.vkCode, pressed);

    if !direct_toggle {
        if let Some(event) = binding_state_win::hook_key_event(kb.vkCode, pressed) {
            if let Ok(guard) = EVENT_TX.try_lock() {
                if let Some(tx) = guard.as_ref() {
                    if let Err(error) = tx.try_send(event) {
                        match error {
                            TrySendError::Full(_) => {
                                warn!(
                                    vk = kb.vkCode,
                                    "game hook event queue full; key event dropped"
                                );
                            }
                            TrySendError::Disconnected(_) => {}
                        }
                    }
                }
            }
        }
    }

    let should_block = current_runtime_config().is_some_and(|(binding, block_system)| {
        block_system && binding_state_win::event_should_block(&binding, kb.vkCode, pressed)
    });

    if should_block {
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}
