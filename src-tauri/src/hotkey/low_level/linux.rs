use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{TrySendError, unbounded};
use tauri::AppHandle;
use tracing::{info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::record::{self, ConnectionExt as RecordExt, CS_ALL_CLIENTS};
use x11rb::protocol::xproto::{ConnectionExt as XprotoExt, KEY_PRESS, KEY_RELEASE};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use crate::error::AppError;
use crate::hotkey::binding::{KeyBinding, KeyEventTarget, KeyToken, ModifierKind};
use crate::hotkey::low_level::common::{join_hook, spawn_worker, EventSender, HookHandle};
use crate::hotkey::normalize::normalize_hotkey;
use crate::hotkey::runtime::KeyEvent;

const KEYCODE_MIN: u8 = 8;
const KEYCODE_COUNT: u8 = 248;

static EVENT_TX: Mutex<Option<EventSender>> = Mutex::new(None);
static ACTIVE_HOOK: Mutex<Option<(HookHandle, Arc<AtomicBool>)>> = Mutex::new(None);

pub fn register(app: &AppHandle, hotkey: &str) -> Result<(), AppError> {
    if crate::injection::linux_wayland::is_wayland_session() {
        return Err(AppError::Hotkey(
            "game mode hotkey is not available on Wayland; use the default hotkey backend".to_string(),
        ));
    }

    unregister();

    let normalized = normalize_hotkey(hotkey);
    let binding = KeyBinding::parse(hotkey)?;
    let (tx, rx) = unbounded();
    *EVENT_TX.lock().expect("hotkey event tx mutex poisoned") = Some(tx);

    let app_for_worker = app.clone();
    let worker_thread = spawn_worker(rx, app_for_worker, binding)?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_hook = Arc::clone(&stop);
    let hook_thread = thread::Builder::new()
        .name("veyro-hotkey-hook".into())
        .spawn(move || hook_thread_main(stop_for_hook))
        .map_err(|error| AppError::Hotkey(format!("hotkey hook thread failed: {error}")))?;

    *ACTIVE_HOOK.lock().expect("hotkey hook mutex poisoned") = Some((
        HookHandle {
            hook_thread,
            worker_thread,
        },
        stop,
    ));

    info!("registered low-level keyboard hotkey: {normalized}");
    Ok(())
}

pub fn unregister() {
    let handle = ACTIVE_HOOK.lock().expect("hotkey hook mutex poisoned").take();
    let Some((hook, stop)) = handle else {
        return;
    };

    stop.store(true, Ordering::Release);
    join_hook(hook);
    *EVENT_TX.lock().expect("hotkey event tx mutex poisoned") = None;
    info!("unregistered low-level keyboard hotkey");
}

fn hook_thread_main(stop: Arc<AtomicBool>) {
    let result = (|| -> Result<(), String> {
        let (conn, _screen_num) = RustConnection::connect(None)
            .map_err(|error| format!("failed to connect to X11: {error}"))?;

        let _version = conn
            .record_query_version(1, 13)
            .map_err(|error| format!("XRecord query failed: {error}"))?
            .reply()
            .map_err(|error| format!("XRecord query reply failed: {error}"))?;

        let context = conn
            .record_create_context(0, &[])
            .map_err(|error| format!("XRecord create context failed: {error}"))?
            .reply()
            .map_err(|error| format!("XRecord create context reply failed: {error}"))?
            .id;

        let range = record::Interval {
            first: KEY_PRESS,
            last: KEY_RELEASE,
        };
        conn.record_register_clients(context, CS_ALL_CLIENTS, &[range], &[])
            .map_err(|error| format!("XRecord register clients failed: {error}"))?
            .check()
            .map_err(|error| format!("XRecord register clients check failed: {error}"))?;

        conn.record_enable_context(context, true)
            .map_err(|error| format!("XRecord enable context failed: {error}"))?
            .check()
            .map_err(|error| format!("XRecord enable context check failed: {error}"))?;

        let keymap = build_keymap(&conn)?;

        while !stop.load(Ordering::Acquire) {
            match conn.poll_for_event() {
                Ok(Some(event)) => {
                    if let Some(key_event) = x11_event_to_key_event(&event, &keymap) {
                        send_event(key_event);
                    }
                }
                Ok(None) => thread::sleep(std::time::Duration::from_millis(5)),
                Err(error) => {
                    return Err(format!("X11 poll failed: {error}"));
                }
            }
        }

        let _ = conn.record_enable_context(context, false);
        Ok(())
    })();

    if let Err(error) = result {
        warn!("linux low-level hotkey hook failed: {error}");
    }
}

fn send_event(event: KeyEvent) {
    if let Ok(guard) = EVENT_TX.try_lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.try_send(event).map_err(|error| match error {
                TrySendError::Full(_) | TrySendError::Disconnected(_) => {}
            });
        }
    }
}

struct Keymap {
    code_to_target: Vec<Option<KeyEventTarget>>,
}

fn build_keymap(conn: &RustConnection) -> Result<Keymap, String> {
    let mapping = conn
        .get_keyboard_mapping(KEYCODE_MIN, KEYCODE_COUNT)
        .map_err(|error| format!("get keyboard mapping failed: {error}"))?
        .reply()
        .map_err(|error| format!("get keyboard mapping reply failed: {error}"))?;

    let mut code_to_target = vec![None; KEYCODE_COUNT as usize];
    for (index, keysyms) in mapping.keysyms.chunks(mapping.keysyms_per_keycode as usize).enumerate() {
        if index >= code_to_target.len() {
            break;
        }
        for keysym in keysyms {
            if *keysym == 0 {
                continue;
            }
            if let Some(target) = keysym_to_target(*keysym) {
                code_to_target[index] = Some(target);
                break;
            }
        }
    }

    Ok(Keymap { code_to_target })
}

fn x11_event_to_key_event(event: &Event, keymap: &Keymap) -> Option<KeyEvent> {
    let (detail, pressed) = match event {
        Event::KeyPress(ev) => (ev.detail, true),
        Event::KeyRelease(ev) => (ev.detail, false),
        _ => return None,
    };

    let index = detail.saturating_sub(KEYCODE_MIN) as usize;
    let target = keymap.code_to_target.get(index).and_then(|entry| *entry)?;
    Some(KeyEvent { target, pressed })
}

fn keysym_to_target(keysym: u32) -> Option<KeyEventTarget> {
    match keysym {
        0xFFE1 | 0xFFE2 => Some(KeyEventTarget::Modifier(ModifierKind::Shift)),
        0xFFE3 | 0xFFE4 => Some(KeyEventTarget::Modifier(ModifierKind::Ctrl)),
        0xFFE9 | 0xFFEA => Some(KeyEventTarget::Modifier(ModifierKind::Alt)),
        0xFFE7 | 0xFFE8 => Some(KeyEventTarget::Modifier(ModifierKind::Meta)),
        0xFFBE => Some(KeyEventTarget::Key(KeyToken::F(1))),
        0xFFBF => Some(KeyEventTarget::Key(KeyToken::F(2))),
        0xFFC0 => Some(KeyEventTarget::Key(KeyToken::F(3))),
        0xFFC1 => Some(KeyEventTarget::Key(KeyToken::F(4))),
        0xFFC2 => Some(KeyEventTarget::Key(KeyToken::F(5))),
        0xFFC3 => Some(KeyEventTarget::Key(KeyToken::F(6))),
        0xFFC4 => Some(KeyEventTarget::Key(KeyToken::F(7))),
        0xFFC5 => Some(KeyEventTarget::Key(KeyToken::F(8))),
        0xFFC6 => Some(KeyEventTarget::Key(KeyToken::F(9))),
        0xFFC7 => Some(KeyEventTarget::Key(KeyToken::F(10))),
        0xFFC8 => Some(KeyEventTarget::Key(KeyToken::F(11))),
        0xFFC9 => Some(KeyEventTarget::Key(KeyToken::F(12))),
        0xFFCA => Some(KeyEventTarget::Key(KeyToken::F(13))),
        0xFFCB => Some(KeyEventTarget::Key(KeyToken::F(14))),
        0xFFCC => Some(KeyEventTarget::Key(KeyToken::F(15))),
        0xFFCD => Some(KeyEventTarget::Key(KeyToken::F(16))),
        0xFFCE => Some(KeyEventTarget::Key(KeyToken::F(17))),
        0xFFCF => Some(KeyEventTarget::Key(KeyToken::F(18))),
        0xFFD0 => Some(KeyEventTarget::Key(KeyToken::F(19))),
        0xFFD1 => Some(KeyEventTarget::Key(KeyToken::F(20))),
        0xFFD2 => Some(KeyEventTarget::Key(KeyToken::F(21))),
        0xFFD3 => Some(KeyEventTarget::Key(KeyToken::F(22))),
        0xFFD4 => Some(KeyEventTarget::Key(KeyToken::F(23))),
        0xFFD5 => Some(KeyEventTarget::Key(KeyToken::F(24))),
        0x20 => Some(KeyEventTarget::Key(KeyToken::Space)),
        0xFF09 => Some(KeyEventTarget::Key(KeyToken::Tab)),
        0xFF0D => Some(KeyEventTarget::Key(KeyToken::Enter)),
        0xFF1B => Some(KeyEventTarget::Key(KeyToken::Escape)),
        0xFF08 => Some(KeyEventTarget::Key(KeyToken::Backspace)),
        0xFFFF => Some(KeyEventTarget::Key(KeyToken::Delete)),
        0xFF63 => Some(KeyEventTarget::Key(KeyToken::Insert)),
        0xFF50 => Some(KeyEventTarget::Key(KeyToken::Home)),
        0xFF57 => Some(KeyEventTarget::Key(KeyToken::End)),
        0xFF55 => Some(KeyEventTarget::Key(KeyToken::PageUp)),
        0xFF56 => Some(KeyEventTarget::Key(KeyToken::PageDown)),
        0xFF52 => Some(KeyEventTarget::Key(KeyToken::ArrowUp)),
        0xFF54 => Some(KeyEventTarget::Key(KeyToken::ArrowDown)),
        0xFF51 => Some(KeyEventTarget::Key(KeyToken::ArrowLeft)),
        0xFF53 => Some(KeyEventTarget::Key(KeyToken::ArrowRight)),
        0xFFE5 => Some(KeyEventTarget::Key(KeyToken::CapsLock)),
        0xFF7F => Some(KeyEventTarget::Key(KeyToken::NumLock)),
        0xFF14 => Some(KeyEventTarget::Key(KeyToken::ScrollLock)),
        0xFFB0..=0xFFB9 => Some(KeyEventTarget::Key(KeyToken::Numpad((keysym - 0xFFB0) as u8))),
        0xFFAB => Some(KeyEventTarget::Key(KeyToken::NumpadAdd)),
        0xFFAD => Some(KeyEventTarget::Key(KeyToken::NumpadSubtract)),
        0xFFAA => Some(KeyEventTarget::Key(KeyToken::NumpadMultiply)),
        0xFFAF => Some(KeyEventTarget::Key(KeyToken::NumpadDivide)),
        0xFFAE => Some(KeyEventTarget::Key(KeyToken::NumpadDecimal)),
        0x0041..=0x005A => Some(KeyEventTarget::Key(KeyToken::Char(keysym as u8))),
        0x0061..=0x007A => Some(KeyEventTarget::Key(KeyToken::Char(
            keysym as u8 - b'a' + b'A',
        ))),
        0x0030..=0x0039 => Some(KeyEventTarget::Key(KeyToken::Char(keysym as u8))),
        _ => None,
    }
}
