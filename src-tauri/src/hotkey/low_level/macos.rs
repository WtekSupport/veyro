use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes, kCFRunLoopDefaultMode};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType, EventField,
};
use crossbeam_channel::{TrySendError, unbounded};
use tauri::AppHandle;
use tracing::{info, warn};

use crate::error::AppError;
use crate::hotkey::binding::{KeyBinding, KeyEventTarget, KeyToken, ModifierKind};
use crate::hotkey::low_level::common::{join_hook, spawn_worker, EventSender, HookHandle};
use crate::hotkey::normalize::normalize_hotkey;
use crate::hotkey::runtime::KeyEvent;

static EVENT_TX: Mutex<Option<EventSender>> = Mutex::new(None);
static ACTIVE_HOOK: Mutex<Option<(HookHandle, Arc<AtomicBool>)>> = Mutex::new(None);

pub fn register(app: &AppHandle, hotkey: &str) -> Result<(), AppError> {
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
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
        ],
        |_proxy, event_type, event| {
            if let Some(key_event) = cg_event_to_key_event(event_type, event) {
                if let Ok(guard) = EVENT_TX.try_lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.try_send(key_event).map_err(|error| match error {
                            TrySendError::Full(_) | TrySendError::Disconnected(_) => {}
                        });
                    }
                }
            }
            Some(event.clone())
        },
    );

    let tap = match tap {
        Ok(tap) => tap,
        Err(error) => {
            warn!(
                "CGEventTapCreate failed ({error:?}); grant Accessibility permission in System Settings"
            );
            return;
        }
    };

    unsafe {
        let run_loop = CFRunLoop::get_current();
        let loop_source = match tap.mach_port.create_runloop_source(0) {
            Ok(source) => source,
            Err(error) => {
                warn!("failed to create run loop source for event tap: {error:?}");
                return;
            }
        };
        run_loop.add_source(&loop_source, kCFRunLoopCommonModes);
        tap.enable();

        while !stop.load(Ordering::Acquire) {
            CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, std::time::Duration::from_millis(200), false);
        }

        run_loop.remove_source(&loop_source, kCFRunLoopCommonModes);
    }
}

fn cg_event_to_key_event(event_type: CGEventType, event: &CGEvent) -> Option<KeyEvent> {
    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
    let flags = event.get_flags();

    match event_type {
        CGEventType::KeyDown => keycode_to_target(keycode).map(|target| KeyEvent {
            target,
            pressed: true,
        }),
        CGEventType::KeyUp => keycode_to_target(keycode).map(|target| KeyEvent {
            target,
            pressed: false,
        }),
        CGEventType::FlagsChanged => modifier_event(keycode, flags),
        _ => None,
    }
}

fn modifier_event(keycode: u16, flags: CGEventFlags) -> Option<KeyEvent> {
    let kind = modifier_kind_for_keycode(keycode)?;
    let pressed = match kind {
        ModifierKind::Ctrl => flags.contains(CGEventFlags::CGEventFlagControl),
        ModifierKind::Shift => flags.contains(CGEventFlags::CGEventFlagShift),
        ModifierKind::Alt => flags.contains(CGEventFlags::CGEventFlagAlternate),
        ModifierKind::Meta => flags.contains(CGEventFlags::CGEventFlagCommand),
    };
    Some(KeyEvent {
        target: KeyEventTarget::Modifier(kind),
        pressed,
    })
}

fn modifier_kind_for_keycode(keycode: u16) -> Option<ModifierKind> {
    match keycode {
        0x3B | 0x3E => Some(ModifierKind::Ctrl),
        0x38 | 0x3C => Some(ModifierKind::Shift),
        0x3A | 0x3D => Some(ModifierKind::Alt),
        0x37 | 0x36 => Some(ModifierKind::Meta),
        _ => None,
    }
}

fn keycode_to_target(keycode: u16) -> Option<KeyEventTarget> {
    if let Some(kind) = modifier_kind_for_keycode(keycode) {
        return Some(KeyEventTarget::Modifier(kind));
    }

    let token = match keycode {
        0x31 => KeyToken::Space,
        0x30 => KeyToken::Tab,
        0x24 => KeyToken::Enter,
        0x35 => KeyToken::Escape,
        0x33 => KeyToken::Backspace,
        0x75 => KeyToken::Delete,
        0x72 => KeyToken::Insert,
        0x73 => KeyToken::Home,
        0x77 => KeyToken::End,
        0x74 => KeyToken::PageUp,
        0x79 => KeyToken::PageDown,
        0x7E => KeyToken::ArrowUp,
        0x7D => KeyToken::ArrowDown,
        0x7B => KeyToken::ArrowLeft,
        0x7C => KeyToken::ArrowRight,
        0x39 => KeyToken::CapsLock,
        0x7A => KeyToken::F(1),
        0x78 => KeyToken::F(2),
        0x63 => KeyToken::F(3),
        0x76 => KeyToken::F(4),
        0x60 => KeyToken::F(5),
        0x61 => KeyToken::F(6),
        0x62 => KeyToken::F(7),
        0x64 => KeyToken::F(8),
        0x65 => KeyToken::F(9),
        0x6D => KeyToken::F(10),
        0x67 => KeyToken::F(11),
        0x6F => KeyToken::F(12),
        0x69 => KeyToken::F(13),
        0x6B => KeyToken::F(14),
        0x71 => KeyToken::F(15),
        0x6A => KeyToken::F(16),
        0x40 => KeyToken::F(17),
        0x4F => KeyToken::F(18),
        0x50 => KeyToken::F(19),
        0x5A => KeyToken::F(20),
        0x00 => KeyToken::Char(b'A'),
        0x01 => KeyToken::Char(b'S'),
        0x02 => KeyToken::Char(b'D'),
        0x03 => KeyToken::Char(b'F'),
        0x04 => KeyToken::Char(b'H'),
        0x05 => KeyToken::Char(b'G'),
        0x06 => KeyToken::Char(b'Z'),
        0x07 => KeyToken::Char(b'X'),
        0x08 => KeyToken::Char(b'C'),
        0x09 => KeyToken::Char(b'V'),
        0x0B => KeyToken::Char(b'B'),
        0x0C => KeyToken::Char(b'Q'),
        0x0D => KeyToken::Char(b'W'),
        0x0E => KeyToken::Char(b'E'),
        0x0F => KeyToken::Char(b'R'),
        0x10 => KeyToken::Char(b'Y'),
        0x11 => KeyToken::Char(b'T'),
        0x12 => KeyToken::Char(b'1'),
        0x13 => KeyToken::Char(b'2'),
        0x14 => KeyToken::Char(b'3'),
        0x15 => KeyToken::Char(b'4'),
        0x17 => KeyToken::Char(b'5'),
        0x16 => KeyToken::Char(b'6'),
        0x1A => KeyToken::Char(b'7'),
        0x1C => KeyToken::Char(b'8'),
        0x19 => KeyToken::Char(b'9'),
        0x1D => KeyToken::Char(b'0'),
        _ => return None,
    };

    Some(KeyEventTarget::Key(token))
}
