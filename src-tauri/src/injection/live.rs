use crate::settings::InjectionMode;

use super::injector::InjectionError;
use super::prepare::prepare_for_live_injection;

const LIVE_INJECTION_MODE: InjectionMode = InjectionMode::Keyboard;

pub fn insert_live_text(text: &str) -> Result<(), InjectionError> {
    if text.is_empty() {
        return Ok(());
    }

    prepare_for_live_injection();

    #[cfg(windows)]
    {
        return super::windows_keyboard::send_unicode_text(text);
    }

    #[cfg(not(windows))]
    {
        super::clipboard::paste_via_clipboard(text)
    }
}

pub fn replace_live_text(previous_chars: u32, text: &str) -> Result<(), InjectionError> {
    if previous_chars > 0 {
        #[cfg(windows)]
        {
            super::windows_keyboard::send_backspaces(previous_chars)?;
        }
        #[cfg(not(windows))]
        {
            super::keyboard_common::send_backspaces(previous_chars)?;
        }
    }

    if text.is_empty() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        return super::windows_keyboard::send_unicode_text(text);
    }

    #[cfg(not(windows))]
    {
        super::clipboard::paste_via_clipboard(text)
    }
}

pub fn delete_live_backward(char_count: u32) -> Result<(), InjectionError> {
    if char_count == 0 {
        return Ok(());
    }

    prepare_for_live_injection();

    #[cfg(windows)]
    {
        return super::windows::WindowsInjector::delete_backward_sync(
            char_count,
            LIVE_INJECTION_MODE,
        );
    }

    #[cfg(not(windows))]
    {
        super::keyboard_common::send_backspaces(char_count)
    }
}
