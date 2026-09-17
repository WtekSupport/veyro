use crate::error::AppError;
use crate::hotkey::normalize::normalize_hotkey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyToken {
    F(u8),
    Char(u8),
    Space,
    Tab,
    Enter,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    CapsLock,
    NumLock,
    ScrollLock,
    Numpad(u8),
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifierKind {
    Ctrl,
    Shift,
    Alt,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventTarget {
    Modifier(ModifierKind),
    Key(KeyToken),
}

/// Parsed keyboard binding for low-level hook matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    pub require_ctrl: bool,
    pub require_shift: bool,
    pub require_alt: bool,
    pub require_meta: bool,
    pub trigger: Option<KeyToken>,
}

impl KeyBinding {
    pub fn parse(input: &str) -> Result<Self, AppError> {
        let normalized = normalize_hotkey(input);
        let parts: Vec<&str> = normalized.split('+').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err(AppError::Hotkey("hotkey must not be empty".to_string()));
        }

        let mut require_ctrl = false;
        let mut require_shift = false;
        let mut require_alt = false;
        let mut require_meta = false;
        let mut trigger: Option<KeyToken> = None;

        for part in parts {
            match part {
                "Ctrl" => require_ctrl = true,
                "Shift" => require_shift = true,
                "Alt" => require_alt = true,
                "Cmd" => require_meta = true,
                "CmdOrCtrl" if cfg!(windows) => require_ctrl = true,
                "CmdOrCtrl" => require_meta = true,
                token => {
                    if trigger.is_some() {
                        return Err(AppError::Hotkey(format!(
                            "hotkey has multiple trigger keys: {normalized}"
                        )));
                    }
                    trigger = Some(parse_key_token(token).ok_or_else(|| {
                        AppError::Hotkey(format!("unsupported hotkey key: {token}"))
                    })?);
                }
            }
        }

        Ok(Self {
            require_ctrl,
            require_shift,
            require_alt,
            require_meta,
            trigger,
        })
    }

    pub fn modifier_only(&self) -> bool {
        self.trigger.is_none()
    }

    /// Toggle/modifier-only bindings need low-level hook edge events; global shortcuts miss them.
    #[cfg(not(windows))]
    pub fn needs_edge_hook(input: &str) -> bool {
        match Self::parse(input) {
            Ok(binding) => binding.uses_latched_or_modifier_only(),
            Err(_) => false,
        }
    }

    pub fn uses_latched_or_modifier_only(&self) -> bool {
        self.modifier_only()
            || matches!(
                self.trigger,
                Some(KeyToken::ScrollLock | KeyToken::CapsLock | KeyToken::NumLock)
            )
    }

    pub fn mods_match(&self, state: &ModifierState) -> bool {
        state.ctrl == self.require_ctrl
            && state.shift == self.require_shift
            && state.alt == self.require_alt
            && state.meta == self.require_meta
    }

    pub fn event_matches_trigger(&self, target: &KeyEventTarget) -> bool {
        if self.modifier_only() {
            self.primary_modifier()
                .is_some_and(|modifier| *target == KeyEventTarget::Modifier(modifier))
        } else {
            match (&self.trigger, target) {
                (Some(key), KeyEventTarget::Key(token)) => key == token,
                _ => false,
            }
        }
    }

    pub fn required_modifier_released(&self, target: &KeyEventTarget) -> bool {
        match target {
            KeyEventTarget::Modifier(ModifierKind::Ctrl) => self.require_ctrl,
            KeyEventTarget::Modifier(ModifierKind::Shift) => self.require_shift,
            KeyEventTarget::Modifier(ModifierKind::Alt) => self.require_alt,
            KeyEventTarget::Modifier(ModifierKind::Meta) => self.require_meta,
            _ => false,
        }
    }

    pub(crate) fn primary_modifier(&self) -> Option<ModifierKind> {
        if !self.modifier_only() {
            return None;
        }
        if self.require_ctrl {
            Some(ModifierKind::Ctrl)
        } else if self.require_shift {
            Some(ModifierKind::Shift)
        } else if self.require_alt {
            Some(ModifierKind::Alt)
        } else if self.require_meta {
            Some(ModifierKind::Meta)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierState {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl ModifierState {
    pub fn set(&mut self, kind: ModifierKind, pressed: bool) {
        match kind {
            ModifierKind::Ctrl => self.ctrl = pressed,
            ModifierKind::Shift => self.shift = pressed,
            ModifierKind::Alt => self.alt = pressed,
            ModifierKind::Meta => self.meta = pressed,
        }
    }
}

fn parse_key_token(token: &str) -> Option<KeyToken> {
    match token {
        "Space" => Some(KeyToken::Space),
        "Tab" => Some(KeyToken::Tab),
        "Enter" => Some(KeyToken::Enter),
        "Escape" => Some(KeyToken::Escape),
        "Backspace" => Some(KeyToken::Backspace),
        "Delete" => Some(KeyToken::Delete),
        "Insert" => Some(KeyToken::Insert),
        "Home" => Some(KeyToken::Home),
        "End" => Some(KeyToken::End),
        "PageUp" => Some(KeyToken::PageUp),
        "PageDown" => Some(KeyToken::PageDown),
        "Up" => Some(KeyToken::ArrowUp),
        "Down" => Some(KeyToken::ArrowDown),
        "Left" => Some(KeyToken::ArrowLeft),
        "Right" => Some(KeyToken::ArrowRight),
        "CapsLock" => Some(KeyToken::CapsLock),
        "NumLock" => Some(KeyToken::NumLock),
        "ScrollLock" => Some(KeyToken::ScrollLock),
        key if key.len() == 1 => {
            let ch = key.chars().next()?.to_ascii_uppercase();
            if ch.is_ascii_alphanumeric() {
                Some(KeyToken::Char(ch as u8))
            } else {
                None
            }
        }
        key if key.len() == 2 && key.starts_with('F') => {
            let num: u8 = key[1..].parse().ok()?;
            (1..=24).contains(&num).then_some(KeyToken::F(num))
        }
        key if key.len() == 7 && key.starts_with("Numpad") => match &key[6..] {
            "0" => Some(KeyToken::Numpad(0)),
            "1" => Some(KeyToken::Numpad(1)),
            "2" => Some(KeyToken::Numpad(2)),
            "3" => Some(KeyToken::Numpad(3)),
            "4" => Some(KeyToken::Numpad(4)),
            "5" => Some(KeyToken::Numpad(5)),
            "6" => Some(KeyToken::Numpad(6)),
            "7" => Some(KeyToken::Numpad(7)),
            "8" => Some(KeyToken::Numpad(8)),
            "9" => Some(KeyToken::Numpad(9)),
            "Add" => Some(KeyToken::NumpadAdd),
            "Subtract" => Some(KeyToken::NumpadSubtract),
            "Multiply" => Some(KeyToken::NumpadMultiply),
            "Divide" => Some(KeyToken::NumpadDivide),
            "Decimal" => Some(KeyToken::NumpadDecimal),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shift_f1() {
        let binding = KeyBinding::parse("Shift+F1").unwrap();
        assert!(binding.require_shift);
        assert!(!binding.modifier_only());
        assert_eq!(binding.trigger, Some(KeyToken::F(1)));
    }

    #[test]
    fn parses_modifier_only_ctrl() {
        let binding = KeyBinding::parse("Ctrl").unwrap();
        assert!(binding.require_ctrl);
        assert!(binding.modifier_only());
    }

    #[test]
    fn latched_and_modifier_only_bindings_need_edge_hook() {
        for hotkey in ["ScrollLock", "CapsLock", "Ctrl"] {
            let binding = KeyBinding::parse(hotkey).unwrap();
            assert!(binding.uses_latched_or_modifier_only(), "{hotkey}");
        }
        let combo = KeyBinding::parse("Shift+F1").unwrap();
        assert!(!combo.uses_latched_or_modifier_only());
    }
}
