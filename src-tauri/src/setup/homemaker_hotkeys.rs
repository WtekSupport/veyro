use crate::hotkey::normalize::normalize_hotkey;

/// Simple-mode push-to-talk keys: dedicated lock keys, unlikely to clash with apps.
pub const PRESETS: &[&str] = &["CapsLock", "ScrollLock"];

pub fn homemaker_hotkey_presets() -> &'static [&'static str] {
    PRESETS
}

pub fn is_homemaker_hotkey_preset(hotkey: &str) -> bool {
    let normalized = normalize_hotkey(hotkey);
    PRESETS.iter().any(|preset| *preset == normalized)
}

pub fn default_homemaker_hotkey() -> &'static str {
    PRESETS[0]
}

pub fn normalize_homemaker_hotkey(settings: &mut crate::settings::AppSettings) -> bool {
    use crate::hotkey::capslock::{self, HOTKEY as CAPSLOCK_HOTKEY};

    let mut changed = false;

    if settings.capslock_ptt {
        if PRESETS.contains(&"CapsLock") && capslock::SUPPORTED {
            return changed;
        }
        settings.capslock_ptt = false;
        changed = true;
    }

    let normalized = normalize_hotkey(&settings.global_hotkey);

    if normalized == "CapsLock" && PRESETS.contains(&"CapsLock") {
        if capslock::SUPPORTED {
            settings.capslock_ptt = true;
            return true;
        }
        if settings.global_hotkey != "CapsLock" {
            settings.global_hotkey = "CapsLock".to_string();
            changed = true;
        }
        return changed;
    }

    if settings.global_hotkey == CAPSLOCK_HOTKEY && PRESETS.contains(&"CapsLock") && capslock::SUPPORTED {
        settings.capslock_ptt = true;
        return true;
    }

    if is_homemaker_hotkey_preset(&normalized) {
        if settings.global_hotkey != normalized {
            settings.global_hotkey = normalized;
            return true;
        }
        return changed;
    }

    settings.global_hotkey = default_homemaker_hotkey().to_string();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn accepts_caps_lock_and_scroll_lock_presets() {
        assert!(is_homemaker_hotkey_preset("CapsLock"));
        assert!(is_homemaker_hotkey_preset("capslock"));
        assert!(is_homemaker_hotkey_preset("ScrollLock"));
        assert!(is_homemaker_hotkey_preset("scrolllock"));
    }

    #[test]
    fn rejects_other_hotkeys() {
        assert!(!is_homemaker_hotkey_preset("Shift+F1"));
        assert!(!is_homemaker_hotkey_preset("Ctrl+Space"));
        assert!(!is_homemaker_hotkey_preset("Ctrl+C"));
    }

    #[test]
    fn normalizes_unknown_hotkey_to_default_preset() {
        let mut settings = AppSettings {
            global_hotkey: "Alt+Cmd+KeyZ".to_string(),
            ..Default::default()
        };
        assert!(normalize_homemaker_hotkey(&mut settings));
        assert_eq!(settings.global_hotkey, "CapsLock");
    }
}
