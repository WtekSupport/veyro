use super::injector::InjectionError;

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

/// Tagged on all Veyro `SendInput` events so the low-level PTT hook can ignore them.
#[cfg(windows)]
pub const INJECTION_TAG: usize = 0x0159_5645_5940;

#[cfg(windows)]
fn injection_extra_info() -> usize {
    INJECTION_TAG
}

#[cfg(windows)]
pub fn is_tagged_injection(extra_info: usize) -> bool {
    extra_info == INJECTION_TAG
}

#[cfg(windows)]
const VK_SCROLL: VIRTUAL_KEY = VIRTUAL_KEY(0x91);
#[cfg(windows)]
const VK_CAPITAL: VIRTUAL_KEY = VIRTUAL_KEY(0x14);
#[cfg(windows)]
const VK_NUMLOCK: VIRTUAL_KEY = VIRTUAL_KEY(0x90);

pub fn send_ctrl_v() -> Result<(), InjectionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL, VK_V,
    };

    unsafe {
        let key_event = |vk: VIRTUAL_KEY, release: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if release {
                        KEYEVENTF_KEYUP
                    } else {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: injection_extra_info(),
                },
            },
        };

        let inputs = [
            key_event(VK_CONTROL, false),
            key_event(VK_V, false),
            key_event(VK_V, true),
            key_event(VK_CONTROL, true),
        ];

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent as usize != inputs.len() {
            return Err(InjectionError::Keyboard(format!(
                "SendInput sent {sent}/{} events",
                inputs.len()
            )));
        }
    }

    Ok(())
}

/// Release Shift/Ctrl/Alt/Meta before injection.
pub fn release_modifiers() -> Result<(), InjectionError> {
    release_modifier_keys(true)
}

/// Release typing modifiers before injection without touching toggle PTT keys.
///
/// Sending KEYUP for ScrollLock/CapsLock/NumLock while the user holds a toggle PTT hotkey
/// makes the low-level hook emit `PttSignal::Released` and stops capture mid-utterance.
pub fn release_typing_modifiers() -> Result<(), InjectionError> {
    release_modifier_keys(false)
}

fn release_modifier_keys(include_toggle_ptt_keys: bool) -> Result<(), InjectionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
        VK_RWIN, VK_SHIFT,
    };

    unsafe {
        let key_up = |vk: VIRTUAL_KEY| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: injection_extra_info(),
                },
            },
        };

        let mut modifiers = vec![
            VK_SHIFT,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_CONTROL,
            VK_LMENU,
            VK_RCONTROL,
            VK_RMENU,
            VK_MENU,
            VK_LWIN,
            VK_RWIN,
        ];

        if include_toggle_ptt_keys {
            modifiers.extend([VK_SCROLL, VK_CAPITAL, VK_NUMLOCK]);
        }

        for vk in modifiers {
            let input = [key_up(vk)];
            let _ = SendInput(&input, std::mem::size_of::<INPUT>() as i32);
        }
    }

    Ok(())
}

pub fn send_unicode_text(text: &str) -> Result<(), InjectionError> {
    if text.is_empty() {
        return Ok(());
    }

    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            let paragraph_break = chars.peek() == Some(&'\n');
            if paragraph_break {
                chars.next();
            }
            send_return()?;
            if paragraph_break {
                send_return()?;
            }
            continue;
        }
        send_unicode_char(ch)?;
    }

    Ok(())
}

fn send_unicode_char(ch: char) -> Result<(), InjectionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };

    unsafe {
        let unicode_event = |scan: u16, release: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: if release {
                        KEYEVENTF_KEYUP | KEYEVENTF_UNICODE
                    } else {
                        KEYEVENTF_UNICODE
                    },
                    time: 0,
                    dwExtraInfo: injection_extra_info(),
                },
            },
        };

        let scan = ch as u16;
        let inputs = [unicode_event(scan, false), unicode_event(scan, true)];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent as usize != inputs.len() {
            return Err(InjectionError::Keyboard(format!(
                "SendInput sent {sent}/{} unicode events",
                inputs.len()
            )));
        }
    }

    Ok(())
}

pub fn send_backspaces(char_count: u32) -> Result<(), InjectionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_BACK,
    };

    if char_count == 0 {
        return Ok(());
    }

    unsafe {
        let key_event = |vk: VIRTUAL_KEY, release: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if release {
                        KEYEVENTF_KEYUP
                    } else {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: injection_extra_info(),
                },
            },
        };

        for _ in 0..char_count {
            let inputs = [key_event(VK_BACK, false), key_event(VK_BACK, true)];
            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            if sent as usize != inputs.len() {
                return Err(InjectionError::Keyboard(format!(
                    "SendInput sent {sent}/{} backspace events",
                    inputs.len()
                )));
            }
        }
    }

    Ok(())
}

/// Move caret to the end of the field (Ctrl+End — end of document in most editors).
pub fn send_ctrl_end() -> Result<(), InjectionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL, VK_END,
    };

    unsafe {
        let key_event = |vk: VIRTUAL_KEY, release: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if release {
                        KEYEVENTF_KEYUP
                    } else {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: injection_extra_info(),
                },
            },
        };

        let inputs = [
            key_event(VK_CONTROL, false),
            key_event(VK_END, false),
            key_event(VK_END, true),
            key_event(VK_CONTROL, true),
        ];

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent as usize != inputs.len() {
            return Err(InjectionError::Keyboard(format!(
                "SendInput sent {sent}/{} ctrl+end events",
                inputs.len()
            )));
        }
    }

    Ok(())
}

pub fn send_return() -> Result<(), InjectionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_RETURN,
    };

    unsafe {
        let key_event = |vk: VIRTUAL_KEY, release: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if release {
                        KEYEVENTF_KEYUP
                    } else {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: injection_extra_info(),
                },
            },
        };

        let inputs = [key_event(VK_RETURN, false), key_event(VK_RETURN, true)];

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent as usize != inputs.len() {
            return Err(InjectionError::Keyboard(format!(
                "SendInput sent {sent}/{} events",
                inputs.len()
            )));
        }
    }

    Ok(())
}
