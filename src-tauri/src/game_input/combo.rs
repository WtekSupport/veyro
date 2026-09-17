use crate::game_input::PttSignal;
use crate::hotkey::binding::{KeyBinding, KeyEventTarget, ModifierState};

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub target: KeyEventTarget,
    pub pressed: bool,
}

pub fn process_combo_event(
    binding: &KeyBinding,
    mods: &mut ModifierState,
    combo_active: &mut bool,
    event: KeyEvent,
    allow_latched_repeat: bool,
) -> Option<PttSignal> {
    if let KeyEventTarget::Modifier(kind) = event.target {
        mods.set(kind, event.pressed);
    }

    if event.pressed {
        let latched_repeat = allow_latched_repeat && binding.uses_latched_or_modifier_only();
        if binding.event_matches_trigger(&event.target)
            && binding.mods_match(mods)
            && (!*combo_active || latched_repeat)
        {
            *combo_active = true;
            return Some(PttSignal::Pressed);
        }
        return None;
    }

    if !*combo_active {
        return None;
    }

    if binding.event_matches_trigger(&event.target)
        || binding.required_modifier_released(&event.target)
    {
        *combo_active = false;
        return Some(PttSignal::Released);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::binding::{KeyBinding, KeyEventTarget, KeyToken, ModifierKind};

    #[test]
    fn shift_f1_press_and_release_on_f1_up() {
        let binding = KeyBinding::parse("Shift+F1").unwrap();
        let mut mods = ModifierState::default();
        let mut active = false;

        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Modifier(ModifierKind::Shift),
                    pressed: true,
                },
                false,
            ),
            None
        );
        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::F(1)),
                    pressed: true,
                },
                false,
            ),
            Some(PttSignal::Pressed)
        );
        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::F(1)),
                    pressed: false,
                },
                false,
            ),
            Some(PttSignal::Released)
        );
    }

    #[test]
    fn shift_f1_release_on_shift_up() {
        let binding = KeyBinding::parse("Shift+F1").unwrap();
        let mut mods = ModifierState::default();
        let mut active = false;

        let _ = process_combo_event(
            &binding,
            &mut mods,
            &mut active,
            KeyEvent {
                target: KeyEventTarget::Modifier(ModifierKind::Shift),
                pressed: true,
            },
            false,
        );
        let _ = process_combo_event(
            &binding,
            &mut mods,
            &mut active,
            KeyEvent {
                target: KeyEventTarget::Key(KeyToken::F(1)),
                pressed: true,
            },
            false,
        );
        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Modifier(ModifierKind::Shift),
                    pressed: false,
                },
                false,
            ),
            Some(PttSignal::Released)
        );
    }

    #[test]
    fn scroll_lock_press_and_release() {
        let binding = KeyBinding::parse("ScrollLock").unwrap();
        let mut mods = ModifierState::default();
        let mut active = false;

        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::ScrollLock),
                    pressed: true,
                },
                false,
            ),
            Some(PttSignal::Pressed)
        );
        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::ScrollLock),
                    pressed: false,
                },
                false,
            ),
            Some(PttSignal::Released)
        );
    }

    #[test]
    fn scroll_lock_toggle_repeat_after_missed_release() {
        let binding = KeyBinding::parse("ScrollLock").unwrap();
        let mut mods = ModifierState::default();
        let mut active = false;

        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::ScrollLock),
                    pressed: true,
                },
                true,
            ),
            Some(PttSignal::Pressed)
        );
        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::ScrollLock),
                    pressed: true,
                },
                true,
            ),
            Some(PttSignal::Pressed)
        );
    }

    #[test]
    fn ignores_repeat_while_combo_active() {
        let binding = KeyBinding::parse("Shift+F1").unwrap();
        let mut mods = ModifierState::default();
        let mut active = false;

        let _ = process_combo_event(
            &binding,
            &mut mods,
            &mut active,
            KeyEvent {
                target: KeyEventTarget::Modifier(ModifierKind::Shift),
                pressed: true,
            },
            false,
        );
        let _ = process_combo_event(
            &binding,
            &mut mods,
            &mut active,
            KeyEvent {
                target: KeyEventTarget::Key(KeyToken::F(1)),
                pressed: true,
            },
            false,
        );
        assert_eq!(
            process_combo_event(
                &binding,
                &mut mods,
                &mut active,
                KeyEvent {
                    target: KeyEventTarget::Key(KeyToken::F(1)),
                    pressed: true,
                },
                false,
            ),
            None
        );
    }
}
