use tauri_plugin_global_shortcut::{Code, Shortcut};

use crate::error::AppError;

/// Normalize user-facing hotkey strings for storage and display.
pub fn normalize_hotkey(input: &str) -> String {
    let parts = split_parts(input);
    if parts.is_empty() {
        return "Ctrl+Shift+Space".to_string();
    }

    parts
        .iter()
        .map(|part| canonical_token(part))
        .collect::<Vec<_>>()
        .join("+")
}

pub fn parse_hotkey(input: &str) -> Result<Shortcut, AppError> {
    let parts = split_parts(input);
    if parts.is_empty() {
        return Err(AppError::Hotkey("hotkey must not be empty".to_string()));
    }

    if parts.len() == 1 {
        if let Some(code) = standalone_code(&parts[0]) {
            return Ok(Shortcut::new(None, code));
        }
    }

    let normalized = normalize_hotkey(input);
    normalized
        .parse::<Shortcut>()
        .map_err(|error| AppError::Hotkey(format!("invalid hotkey {normalized}: {error}")))
}

fn split_parts(input: &str) -> Vec<String> {
    input
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn standalone_code(token: &str) -> Option<Code> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => Some(Code::ControlLeft),
        "shift" => Some(Code::ShiftLeft),
        "alt" | "option" => Some(Code::AltLeft),
        "cmd" | "command" | "super" | "win" | "windows" => Some(Code::MetaLeft),
        "capslock" | "caps" => Some(Code::CapsLock),
        "numlock" => Some(Code::NumLock),
        "scrolllock" => Some(Code::ScrollLock),
        _ => None,
    }
}

fn canonical_token(token: &str) -> String {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => "Ctrl".to_string(),
        "shift" => "Shift".to_string(),
        "alt" | "option" => "Alt".to_string(),
        "cmd" | "command" | "super" | "win" | "windows" => "Cmd".to_string(),
        "cmdorctrl" | "commandorcontrol" | "commandorctrl" => "CmdOrCtrl".to_string(),
        "space" | "spacebar" => "Space".to_string(),
        "capslock" | "caps" => "CapsLock".to_string(),
        "numlock" => "NumLock".to_string(),
        "scrolllock" => "ScrollLock".to_string(),
        "tab" => "Tab".to_string(),
        "enter" | "return" => "Enter".to_string(),
        "escape" | "esc" => "Escape".to_string(),
        "backspace" => "Backspace".to_string(),
        "delete" | "del" => "Delete".to_string(),
        "insert" | "ins" => "Insert".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "pageup" => "PageUp".to_string(),
        "pagedown" => "PageDown".to_string(),
        "up" | "arrowup" => "Up".to_string(),
        "down" | "arrowdown" => "Down".to_string(),
        "left" | "arrowleft" => "Left".to_string(),
        "right" | "arrowright" => "Right".to_string(),
        key if key.len() == 1 => key.to_ascii_uppercase(),
        key if key.len() == 2 && key.starts_with('f') && key[1..].chars().all(|c| c.is_ascii_digit()) => {
            key.to_ascii_uppercase()
        }
        key if key.starts_with("key") && key.len() == 4 => key[3..4].to_ascii_uppercase(),
        key if key.starts_with("digit") && key.len() == 6 => key[5..6].to_string(),
        key if key.starts_with("numpad") => {
            let suffix = &key[6..];
            if suffix.chars().all(|c| c.is_ascii_digit()) && suffix.len() == 1 {
                format!("Numpad{suffix}")
            } else {
                format!("Numpad{}", suffix.to_ascii_uppercase())
            }
        }
        _ => token.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_ctrl_space() {
        assert_eq!(normalize_hotkey("Ctrl+Space"), "Ctrl+Space");
        assert_eq!(normalize_hotkey("ctrl+space"), "Ctrl+Space");
        assert_eq!(normalize_hotkey("Control+Space"), "Ctrl+Space");
    }

    #[test]
    fn normalizes_shift_combo() {
        assert_eq!(normalize_hotkey("Ctrl+Shift+Space"), "Ctrl+Shift+Space");
        assert_eq!(normalize_hotkey("Shift+F1"), "Shift+F1");
    }

    #[test]
    fn normalizes_single_modifier_and_capslock() {
        assert_eq!(normalize_hotkey("Ctrl"), "Ctrl");
        assert_eq!(normalize_hotkey("capslock"), "CapsLock");
    }

    #[test]
    fn parses_single_modifier_and_capslock() {
        assert!(parse_hotkey("Ctrl").is_ok());
        assert!(parse_hotkey("Shift").is_ok());
        assert!(parse_hotkey("Alt").is_ok());
        assert!(parse_hotkey("Cmd").is_ok());
        assert!(parse_hotkey("CapsLock").is_ok());
    }
}
