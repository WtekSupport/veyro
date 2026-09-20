use std::thread;
use std::time::Duration;

use super::focus_target::{capture_injection_target, restore_injection_target};
use super::timing::FOCUS_BEFORE_INJECT_MS;

/// Restore the captured target window and clear held modifier keys before live injection.
///
/// PTT hotkeys often keep Shift/Ctrl/Alt down while recording; that breaks paste and can
/// swallow simulated keystrokes in the target field.
pub fn prepare_for_live_injection() {
    capture_injection_target();
    restore_injection_target();
    #[cfg(windows)]
    {
        let _ = super::windows_keyboard::release_typing_modifiers();
    }
    thread::sleep(Duration::from_millis(FOCUS_BEFORE_INJECT_MS));
}
