use async_trait::async_trait;

use crate::settings::InjectionMode;

use super::clipboard::paste_via_clipboard;
use super::focus_target::{capture_injection_target, restore_injection_target};
use super::timing::{FOCUS_BEFORE_ENTER_MS, FOCUS_BEFORE_INJECT_MS};
use super::injector::{InjectionBackendInfo, InjectionError, TextInjector};

pub struct WindowsInjector;

impl WindowsInjector {
    pub fn new() -> Self {
        Self
    }

    pub fn insert_text_sync(text: &str, mode: InjectionMode) -> Result<(), InjectionError> {
        match mode {
            InjectionMode::Keyboard => Self::send_unicode_text(text),
            InjectionMode::Paste => paste_via_clipboard(text),
            InjectionMode::Auto => {
                paste_via_clipboard(text).or_else(|_| Self::send_unicode_text(text))
            }
        }
    }

    pub fn delete_backward_sync(char_count: u32, mode: InjectionMode) -> Result<(), InjectionError> {
        match mode {
            InjectionMode::Keyboard => super::windows_keyboard::send_backspaces(char_count),
            InjectionMode::Paste | InjectionMode::Auto => {
                super::windows_keyboard::send_backspaces(char_count)
            }
        }
    }

    fn send_unicode_text(text: &str) -> Result<(), InjectionError> {
        use enigo::{Enigo, Key, Keyboard, Settings};

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|error| InjectionError::Keyboard(error.to_string()))?;

        for ch in text.chars() {
            enigo
                .key(Key::Unicode(ch), enigo::Direction::Click)
                .map_err(|error| InjectionError::Keyboard(error.to_string()))?;
        }

        Ok(())
    }
}

#[async_trait]
impl TextInjector for WindowsInjector {
    async fn insert_text(&self, text: &str, mode: InjectionMode) -> Result<(), InjectionError> {
        capture_injection_target();
        restore_injection_target();
        std::thread::sleep(std::time::Duration::from_millis(FOCUS_BEFORE_INJECT_MS));

        match mode {
            InjectionMode::Keyboard => Self::send_unicode_text(text),
            InjectionMode::Paste => paste_via_clipboard(text),
            InjectionMode::Auto => {
                paste_via_clipboard(text).or_else(|_| Self::send_unicode_text(text))
            }
        }
    }

    async fn delete_backward(&self, char_count: u32, mode: InjectionMode) -> Result<(), InjectionError> {
        capture_injection_target();
        restore_injection_target();
        std::thread::sleep(std::time::Duration::from_millis(FOCUS_BEFORE_INJECT_MS));

        match mode {
            InjectionMode::Keyboard => super::windows_keyboard::send_backspaces(char_count),
            InjectionMode::Paste | InjectionMode::Auto => {
                super::windows_keyboard::send_backspaces(char_count)
            }
        }
    }

    async fn send_enter(&self) -> Result<(), InjectionError> {
        capture_injection_target();
        restore_injection_target();
        std::thread::sleep(std::time::Duration::from_millis(FOCUS_BEFORE_ENTER_MS));
        super::windows_keyboard::send_return()
    }

    fn backend_info(&self) -> InjectionBackendInfo {
        InjectionBackendInfo {
            available: true,
            backend: "windows".to_string(),
        }
    }
}
