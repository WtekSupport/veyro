use async_trait::async_trait;

use crate::settings::InjectionMode;

use super::clipboard::paste_via_clipboard;
use super::injector::{InjectionBackendInfo, InjectionError, TextInjector};

pub struct X11Injector;

impl X11Injector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TextInjector for X11Injector {
    async fn insert_text(&self, text: &str, mode: InjectionMode) -> Result<(), InjectionError> {
        match mode {
            InjectionMode::Keyboard | InjectionMode::Auto | InjectionMode::Paste => {
                paste_via_clipboard(text)
            }
        }
    }

    async fn delete_backward(&self, char_count: u32, _mode: InjectionMode) -> Result<(), InjectionError> {
        super::keyboard_common::send_backspaces(char_count)
    }

    async fn send_enter(&self) -> Result<(), InjectionError> {
        std::thread::sleep(std::time::Duration::from_millis(50));
        super::keyboard_common::send_return()
    }

    fn backend_info(&self) -> InjectionBackendInfo {
        InjectionBackendInfo {
            available: true,
            backend: "linux-x11".to_string(),
        }
    }
}
