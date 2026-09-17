use async_trait::async_trait;

use crate::settings::InjectionMode;

use super::clipboard::paste_via_clipboard;
use super::injector::{InjectionBackendInfo, InjectionError, TextInjector};

pub fn is_wayland_session() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        && std::env::var("XDG_SESSION_TYPE")
            .map(|value| value.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}

pub struct WaylandInjector {
    paste_available: bool,
}

impl Default for WaylandInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl WaylandInjector {
    pub fn new() -> Self {
        Self {
            paste_available: std::process::Command::new("wl-paste")
                .arg("--version")
                .output()
                .is_ok(),
        }
    }
}

#[async_trait]
impl TextInjector for WaylandInjector {
    async fn insert_text(&self, text: &str, mode: InjectionMode) -> Result<(), InjectionError> {
        if !self.paste_available {
            return Err(InjectionError::Unavailable);
        }

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
            available: self.paste_available,
            backend: if self.paste_available {
                "linux-wayland-paste".to_string()
            } else {
                "linux-wayland-unavailable".to_string()
            },
        }
    }
}
