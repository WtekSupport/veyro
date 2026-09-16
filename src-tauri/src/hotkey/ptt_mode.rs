use crate::settings::AppSettings;

/// True when PTT should start on key down and stop on the next key down (not on key up).
pub fn press_to_toggle(settings: &AppSettings) -> bool {
    !settings.ptt_hold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn hold_off_is_always_toggle() {
        let settings = AppSettings {
            ptt_hold: false,
            global_hotkey: "F8".to_string(),
            ..Default::default()
        };
        assert!(press_to_toggle(&settings));
    }

    #[test]
    fn hold_on_scroll_lock_uses_hold() {
        let settings = AppSettings {
            ptt_hold: true,
            global_hotkey: "ScrollLock".to_string(),
            ..Default::default()
        };
        assert!(!press_to_toggle(&settings));
    }

    #[test]
    fn hold_on_f8_is_not_toggle() {
        let settings = AppSettings {
            ptt_hold: true,
            global_hotkey: "F8".to_string(),
            ..Default::default()
        };
        assert!(!press_to_toggle(&settings));
    }
}
