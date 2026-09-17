use std::thread;
use std::time::Duration;

use arboard::Clipboard;

use super::timing::{CLIPBOARD_AFTER_PASTE_MS, CLIPBOARD_BEFORE_PASTE_MS};
#[cfg(not(windows))]
use enigo::{Direction, Key, Keyboard};
use tracing::warn;

use super::injector::InjectionError;

pub struct ClipboardGuard {
    previous: Option<String>,
}

impl ClipboardGuard {
    pub fn capture() -> Result<Self, InjectionError> {
        let mut clipboard = Clipboard::new().map_err(|error| InjectionError::Clipboard(error.to_string()))?;
        let previous = clipboard.get_text().ok();
        Ok(Self { previous })
    }

    pub fn set_text(text: &str) -> Result<(), InjectionError> {
        let mut clipboard = Clipboard::new().map_err(|error| InjectionError::Clipboard(error.to_string()))?;
        clipboard
            .set_text(text)
            .map_err(|error| InjectionError::Clipboard(error.to_string()))
    }

    pub fn restore(self) {
        if let Some(previous) = self.previous {
            if let Err(error) = Self::set_text(&previous) {
                warn!("failed to restore clipboard: {error}");
            }
        }
    }
}

pub fn paste_via_clipboard(text: &str) -> Result<(), InjectionError> {
    let guard = ClipboardGuard::capture()?;
    ClipboardGuard::set_text(text)?;

    thread::sleep(Duration::from_millis(CLIPBOARD_BEFORE_PASTE_MS));

    send_paste_shortcut()?;

    thread::sleep(Duration::from_millis(CLIPBOARD_AFTER_PASTE_MS));
    guard.restore();
    Ok(())
}

#[cfg(windows)]
fn send_paste_shortcut() -> Result<(), InjectionError> {
    super::windows_keyboard::send_ctrl_v()
}

/// Virtual keycode of the physical V key (`kVK_ANSI_V`).
#[cfg(target_os = "macos")]
const KVK_ANSI_V: u32 = 0x09;

#[cfg(target_os = "macos")]
fn send_paste_shortcut() -> Result<(), InjectionError> {
    // Physical V, not `Key::Unicode('v')`: enigo resolves Unicode keys through the active layout
    // and falls back to keycode 0 (A) when it has no "v" (e.g. Russian), sending Cmd+A.
    super::keyboard_common::with_enigo(|enigo| {
        enigo
            .key(Key::Meta, Direction::Press)
            .map_err(|e| InjectionError::Keyboard(e.to_string()))?;
        enigo
            .key(Key::Other(KVK_ANSI_V), Direction::Click)
            .map_err(|e| InjectionError::Keyboard(e.to_string()))?;
        enigo
            .key(Key::Meta, Direction::Release)
            .map_err(|e| InjectionError::Keyboard(e.to_string()))?;
        Ok(())
    })
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn send_paste_shortcut() -> Result<(), InjectionError> {
    super::keyboard_common::with_enigo(|enigo| {
        enigo
            .key(Key::Control, Direction::Press)
            .map_err(|e| InjectionError::Keyboard(e.to_string()))?;
        enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|e| InjectionError::Keyboard(e.to_string()))?;
        enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| InjectionError::Keyboard(e.to_string()))?;
        Ok(())
    })
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use arboard::Clipboard;

    use super::ClipboardGuard;

    #[test]
    fn capture_and_restore_roundtrip() {
        let original = Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
        let text = "voice-input-clipboard-test";
        ClipboardGuard::set_text(text).unwrap();
        let guard = ClipboardGuard::capture().unwrap();
        assert_eq!(guard.previous.as_deref(), Some(text));
        guard.restore();
        if let Some(original) = original {
            let restored = Clipboard::new().unwrap().get_text().unwrap();
            assert_eq!(restored, original);
        }
    }
}
