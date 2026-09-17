//! macOS: Caps Lock as the push-to-talk key.
//!
//! Caps Lock is remapped to F18 — a key no Apple keyboard has, so nothing else listens for it —
//! through the per-session HID key mapping that `hidutil` manages. Other user remappings are
//! kept. The mapping lasts until reboot, so it is re-applied on every launch and removed when the
//! setting is turned off or Veyro quits.

use std::sync::atomic::{AtomicBool, Ordering};

/// Hotkey the global-shortcut plugin sees for the remapped Caps Lock.
pub const HOTKEY: &str = "F18";

/// Whether this platform can remap Caps Lock; the settings toggle is shown only where true.
pub const SUPPORTED: bool = cfg!(target_os = "macos");

/// HID usages (page 0x07) of Caps Lock and F18.
const CAPS_LOCK: u64 = 0x7_0000_0039;
const F18: u64 = 0x7_0000_006D;

/// Set while this session's remapping is in place, so quitting only undoes our own change.
static APPLIED: AtomicBool = AtomicBool::new(false);

/// Add or remove the Caps Lock → F18 remapping, keeping the user's other remappings.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if !SUPPORTED {
        return Ok(());
    }

    let current = parse_user_key_mapping(&hidutil(&["property", "--get", "UserKeyMapping"])?);
    let entries: Vec<_> = with_capslock(current, enabled)
        .into_iter()
        .map(|(src, dst)| {
            serde_json::json!({
                "HIDKeyboardModifierMappingSrc": src,
                "HIDKeyboardModifierMappingDst": dst,
            })
        })
        .collect();
    let payload = serde_json::json!({ "UserKeyMapping": entries }).to_string();
    hidutil(&["property", "--set", &payload])?;

    APPLIED.store(enabled, Ordering::SeqCst);
    Ok(())
}

/// Undo this session's remapping on exit so Caps Lock works normally without Veyro.
pub fn restore_on_exit() {
    if APPLIED.load(Ordering::SeqCst) {
        if let Err(error) = set_enabled(false) {
            tracing::warn!("failed to restore Caps Lock: {error}");
        }
    }
}

fn hidutil(args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("/usr/bin/hidutil")
        .args(args)
        .output()
        .map_err(|error| format!("hidutil: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "hidutil {}: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn with_capslock(mut mappings: Vec<(u64, u64)>, enabled: bool) -> Vec<(u64, u64)> {
    if enabled {
        // A key has a single mapping, so ours replaces any other Caps Lock remap.
        mappings.retain(|(src, _)| *src != CAPS_LOCK);
        mappings.push((CAPS_LOCK, F18));
    } else {
        mappings.retain(|mapping| *mapping != (CAPS_LOCK, F18));
    }
    mappings
}

/// Parse `hidutil property --get UserKeyMapping`, which prints an NSArray description:
/// `(null)`, or `( { HIDKeyboardModifierMappingDst = 30064771181; HIDKeyboardModifierMappingSrc = 30064771129; } )`.
fn parse_user_key_mapping(output: &str) -> Vec<(u64, u64)> {
    let mut mappings = Vec::new();
    for entry in output.split('{').skip(1) {
        let body = entry.split('}').next().unwrap_or_default();
        let (mut src, mut dst) = (None, None);
        for field in body.split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = parse_number(value.trim());
            match key.trim() {
                "HIDKeyboardModifierMappingSrc" => src = value,
                "HIDKeyboardModifierMappingDst" => dst = value,
                _ => {}
            }
        }
        if let (Some(src), Some(dst)) = (src, dst) {
            mappings.push((src, dst));
        }
    }
    mappings
}

fn parse_number(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIGHT_CMD_TO_ESC: (u64, u64) = (0x7_0000_00E7, 0x7_0000_0029);

    #[test]
    fn parses_real_hidutil_output() {
        let output = "(\n        {\n        HIDKeyboardModifierMappingDst = 30064771181;\n        HIDKeyboardModifierMappingSrc = 30064771129;\n    }\n)\n";
        assert_eq!(parse_user_key_mapping(output), vec![(CAPS_LOCK, F18)]);
    }

    #[test]
    fn parses_empty_mapping() {
        assert!(parse_user_key_mapping("(null)\n").is_empty());
        assert!(parse_user_key_mapping("(\n)\n").is_empty());
    }

    #[test]
    fn parses_several_entries_in_any_order_and_hex() {
        let output = "( { HIDKeyboardModifierMappingSrc = 0x7000000E7; HIDKeyboardModifierMappingDst = 0x700000029; }, { HIDKeyboardModifierMappingDst = 30064771181; HIDKeyboardModifierMappingSrc = 30064771129; } )";
        assert_eq!(
            parse_user_key_mapping(output),
            vec![RIGHT_CMD_TO_ESC, (CAPS_LOCK, F18)]
        );
    }

    #[test]
    fn enabling_keeps_other_mappings_and_replaces_capslock_remap() {
        let caps_to_esc = (CAPS_LOCK, 0x7_0000_0029);
        assert_eq!(
            with_capslock(vec![RIGHT_CMD_TO_ESC, caps_to_esc], true),
            vec![RIGHT_CMD_TO_ESC, (CAPS_LOCK, F18)]
        );
    }

    #[test]
    fn disabling_removes_only_our_mapping() {
        assert_eq!(
            with_capslock(vec![RIGHT_CMD_TO_ESC, (CAPS_LOCK, F18)], false),
            vec![RIGHT_CMD_TO_ESC]
        );
    }
}
