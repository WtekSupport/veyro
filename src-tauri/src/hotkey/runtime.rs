use tauri::AppHandle;

use crate::hotkey::binding::{KeyBinding, KeyEventTarget, ModifierState};
use crate::hotkey::manager::handle_shortcut_event;

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub target: KeyEventTarget,
    pub pressed: bool,
}

pub fn process_key_event(
    binding: &KeyBinding,
    mods: &mut ModifierState,
    combo_active: &mut bool,
    app: &AppHandle,
    event: KeyEvent,
) {
    if let KeyEventTarget::Modifier(kind) = event.target {
        mods.set(kind, event.pressed);
    }

    if event.pressed {
        if binding.event_matches_trigger(&event.target)
            && binding.mods_match(mods)
            && !*combo_active
        {
            *combo_active = true;
            handle_shortcut_event(app, true, false);
        }
        return;
    }

    if !*combo_active {
        return;
    }

    if binding.event_matches_trigger(&event.target)
        || binding.required_modifier_released(&event.target)
    {
        *combo_active = false;
        handle_shortcut_event(app, false, true);
    }
}
