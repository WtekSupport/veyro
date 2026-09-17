use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_ADD, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DECIMAL, VK_DELETE, VK_DIVIDE,
    VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F13, VK_F14, VK_F15, VK_F16,
    VK_F17, VK_F18, VK_F19, VK_F2, VK_F20, VK_F21, VK_F22, VK_F23, VK_F24, VK_F3, VK_F4, VK_F5,
    VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_LEFT, VK_MENU, VK_MULTIPLY, VK_NEXT, VK_NUMLOCK, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2,
    VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9, VK_PRIOR,
    VK_RETURN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_RIGHT, VK_SCROLL, VK_SHIFT, VK_SPACE,
    VK_SUBTRACT, VK_TAB,
    VK_UP, VIRTUAL_KEY,
};

use crate::game_input::combo::KeyEvent;
use crate::hotkey::binding::{KeyBinding, KeyToken, ModifierKind, ModifierState};

pub fn read_modifier_state() -> ModifierState {
    ModifierState {
        ctrl: is_vk_down(VK_CONTROL) || is_vk_down(VK_LCONTROL) || is_vk_down(VK_RCONTROL),
        shift: is_vk_down(VK_SHIFT) || is_vk_down(VK_LSHIFT) || is_vk_down(VK_RSHIFT),
        alt: is_vk_down(VK_MENU) || is_vk_down(VK_LMENU) || is_vk_down(VK_RMENU),
        meta: is_vk_down(VK_LWIN) || is_vk_down(VK_RWIN),
    }
}

pub fn binding_pressed(binding: &KeyBinding) -> bool {
    let mods = read_modifier_state();
    if !binding.mods_match(&mods) {
        return false;
    }

    if binding.modifier_only() {
        return binding
            .primary_modifier()
            .is_some_and(|kind| modifier_down(kind));
    }

    binding
        .trigger
        .is_some_and(|token| trigger_down(token))
}

pub fn hook_key_event(vk: u32, pressed: bool) -> Option<KeyEvent> {
    vk_to_target(vk).map(|target| KeyEvent { target, pressed })
}

pub fn event_should_block(binding: &KeyBinding, vk: u32, pressed: bool) -> bool {
    if !pressed {
        return false;
    }

    if let Some(target) = vk_to_target(vk) {
        if binding.event_matches_trigger(&target) {
            return true;
        }
    }

    false
}

fn modifier_down(kind: ModifierKind) -> bool {
    match kind {
        ModifierKind::Ctrl => {
            is_vk_down(VK_CONTROL) || is_vk_down(VK_LCONTROL) || is_vk_down(VK_RCONTROL)
        }
        ModifierKind::Shift => {
            is_vk_down(VK_SHIFT) || is_vk_down(VK_LSHIFT) || is_vk_down(VK_RSHIFT)
        }
        ModifierKind::Alt => is_vk_down(VK_MENU) || is_vk_down(VK_LMENU) || is_vk_down(VK_RMENU),
        ModifierKind::Meta => is_vk_down(VK_LWIN) || is_vk_down(VK_RWIN),
    }
}

fn trigger_down(token: KeyToken) -> bool {
    token_to_vk(token).is_some_and(is_vk_down)
}

fn is_vk_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 }
}

fn token_to_vk(token: KeyToken) -> Option<VIRTUAL_KEY> {
    Some(match token {
        KeyToken::Space => VK_SPACE,
        KeyToken::Tab => VK_TAB,
        KeyToken::Enter => VK_RETURN,
        KeyToken::Escape => VK_ESCAPE,
        KeyToken::Backspace => VK_BACK,
        KeyToken::Delete => VK_DELETE,
        KeyToken::Insert => VK_INSERT,
        KeyToken::Home => VK_HOME,
        KeyToken::End => VK_END,
        KeyToken::PageUp => VK_PRIOR,
        KeyToken::PageDown => VK_NEXT,
        KeyToken::ArrowUp => VK_UP,
        KeyToken::ArrowDown => VK_DOWN,
        KeyToken::ArrowLeft => VK_LEFT,
        KeyToken::ArrowRight => VK_RIGHT,
        KeyToken::CapsLock => VK_CAPITAL,
        KeyToken::NumLock => VK_NUMLOCK,
        KeyToken::ScrollLock => VK_SCROLL,
        KeyToken::F(n) => f_key(n)?,
        KeyToken::Numpad(n) => numpad_digit(n)?,
        KeyToken::NumpadAdd => VK_ADD,
        KeyToken::NumpadSubtract => VK_SUBTRACT,
        KeyToken::NumpadMultiply => VK_MULTIPLY,
        KeyToken::NumpadDivide => VK_DIVIDE,
        KeyToken::NumpadDecimal => VK_DECIMAL,
        KeyToken::Char(ch) if ch.is_ascii_alphanumeric() => VIRTUAL_KEY(ch as u16),
        _ => return None,
    })
}

fn f_key(n: u8) -> Option<VIRTUAL_KEY> {
    Some(match n {
        1 => VK_F1,
        2 => VK_F2,
        3 => VK_F3,
        4 => VK_F4,
        5 => VK_F5,
        6 => VK_F6,
        7 => VK_F7,
        8 => VK_F8,
        9 => VK_F9,
        10 => VK_F10,
        11 => VK_F11,
        12 => VK_F12,
        13 => VK_F13,
        14 => VK_F14,
        15 => VK_F15,
        16 => VK_F16,
        17 => VK_F17,
        18 => VK_F18,
        19 => VK_F19,
        20 => VK_F20,
        21 => VK_F21,
        22 => VK_F22,
        23 => VK_F23,
        24 => VK_F24,
        _ => return None,
    })
}

fn numpad_digit(n: u8) -> Option<VIRTUAL_KEY> {
    Some(match n {
        0 => VK_NUMPAD0,
        1 => VK_NUMPAD1,
        2 => VK_NUMPAD2,
        3 => VK_NUMPAD3,
        4 => VK_NUMPAD4,
        5 => VK_NUMPAD5,
        6 => VK_NUMPAD6,
        7 => VK_NUMPAD7,
        8 => VK_NUMPAD8,
        9 => VK_NUMPAD9,
        _ => return None,
    })
}

use crate::hotkey::binding::KeyEventTarget;

fn vk_to_target(vk: u32) -> Option<KeyEventTarget> {
    match vk {
        x if x == VK_LCONTROL.0 as u32 || x == VK_RCONTROL.0 as u32 || x == VK_CONTROL.0 as u32 => {
            Some(KeyEventTarget::Modifier(ModifierKind::Ctrl))
        }
        x if x == VK_LSHIFT.0 as u32 || x == VK_RSHIFT.0 as u32 || x == VK_SHIFT.0 as u32 => {
            Some(KeyEventTarget::Modifier(ModifierKind::Shift))
        }
        x if x == VK_LMENU.0 as u32 || x == VK_RMENU.0 as u32 || x == VK_MENU.0 as u32 => {
            Some(KeyEventTarget::Modifier(ModifierKind::Alt))
        }
        x if x == VK_LWIN.0 as u32 || x == VK_RWIN.0 as u32 => {
            Some(KeyEventTarget::Modifier(ModifierKind::Meta))
        }
        x if x == VK_SPACE.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Space)),
        x if x == VK_TAB.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Tab)),
        x if x == VK_RETURN.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Enter)),
        x if x == VK_ESCAPE.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Escape)),
        x if x == VK_BACK.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Backspace)),
        x if x == VK_DELETE.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Delete)),
        x if x == VK_INSERT.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Insert)),
        x if x == VK_HOME.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Home)),
        x if x == VK_END.0 as u32 => Some(KeyEventTarget::Key(KeyToken::End)),
        x if x == VK_PRIOR.0 as u32 => Some(KeyEventTarget::Key(KeyToken::PageUp)),
        x if x == VK_NEXT.0 as u32 => Some(KeyEventTarget::Key(KeyToken::PageDown)),
        x if x == VK_UP.0 as u32 => Some(KeyEventTarget::Key(KeyToken::ArrowUp)),
        x if x == VK_DOWN.0 as u32 => Some(KeyEventTarget::Key(KeyToken::ArrowDown)),
        x if x == VK_LEFT.0 as u32 => Some(KeyEventTarget::Key(KeyToken::ArrowLeft)),
        x if x == VK_RIGHT.0 as u32 => Some(KeyEventTarget::Key(KeyToken::ArrowRight)),
        x if x == VK_CAPITAL.0 as u32 => Some(KeyEventTarget::Key(KeyToken::CapsLock)),
        x if x == VK_NUMLOCK.0 as u32 => Some(KeyEventTarget::Key(KeyToken::NumLock)),
        x if x == VK_SCROLL.0 as u32 => Some(KeyEventTarget::Key(KeyToken::ScrollLock)),
        x if x == VK_F1.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(1))),
        x if x == VK_F2.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(2))),
        x if x == VK_F3.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(3))),
        x if x == VK_F4.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(4))),
        x if x == VK_F5.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(5))),
        x if x == VK_F6.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(6))),
        x if x == VK_F7.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(7))),
        x if x == VK_F8.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(8))),
        x if x == VK_F9.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(9))),
        x if x == VK_F10.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(10))),
        x if x == VK_F11.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(11))),
        x if x == VK_F12.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(12))),
        x if x == VK_F13.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(13))),
        x if x == VK_F14.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(14))),
        x if x == VK_F15.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(15))),
        x if x == VK_F16.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(16))),
        x if x == VK_F17.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(17))),
        x if x == VK_F18.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(18))),
        x if x == VK_F19.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(19))),
        x if x == VK_F20.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(20))),
        x if x == VK_F21.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(21))),
        x if x == VK_F22.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(22))),
        x if x == VK_F23.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(23))),
        x if x == VK_F24.0 as u32 => Some(KeyEventTarget::Key(KeyToken::F(24))),
        x if x == VK_NUMPAD0.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(0))),
        x if x == VK_NUMPAD1.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(1))),
        x if x == VK_NUMPAD2.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(2))),
        x if x == VK_NUMPAD3.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(3))),
        x if x == VK_NUMPAD4.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(4))),
        x if x == VK_NUMPAD5.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(5))),
        x if x == VK_NUMPAD6.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(6))),
        x if x == VK_NUMPAD7.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(7))),
        x if x == VK_NUMPAD8.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(8))),
        x if x == VK_NUMPAD9.0 as u32 => Some(KeyEventTarget::Key(KeyToken::Numpad(9))),
        x if x == VK_ADD.0 as u32 => Some(KeyEventTarget::Key(KeyToken::NumpadAdd)),
        x if x == VK_SUBTRACT.0 as u32 => Some(KeyEventTarget::Key(KeyToken::NumpadSubtract)),
        x if x == VK_MULTIPLY.0 as u32 => Some(KeyEventTarget::Key(KeyToken::NumpadMultiply)),
        x if x == VK_DIVIDE.0 as u32 => Some(KeyEventTarget::Key(KeyToken::NumpadDivide)),
        x if x == VK_DECIMAL.0 as u32 => Some(KeyEventTarget::Key(KeyToken::NumpadDecimal)),
        x if (0x41..=0x5A).contains(&x) || (0x30..=0x39).contains(&x) => {
            Some(KeyEventTarget::Key(KeyToken::Char(x as u8)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_modifier_state_without_panic() {
        let _ = read_modifier_state();
    }
}
