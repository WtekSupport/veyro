use std::sync::Arc;

use async_trait::async_trait;

use crate::settings::InjectionMode;

#[derive(Debug, thiserror::Error)]
pub enum InjectionError {
    #[error("text injection unavailable on this platform")]
    Unavailable,
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("keyboard simulation failed: {0}")]
    Keyboard(String),
    #[error("permission denied")]
    PermissionDenied,
}

#[derive(Debug, Clone)]
pub struct InjectionBackendInfo {
    pub available: bool,
    pub backend: String,
}

#[async_trait]
pub trait TextInjector: Send + Sync {
    async fn insert_text(&self, text: &str, mode: InjectionMode) -> Result<(), InjectionError>;
    async fn delete_backward(&self, char_count: u32, mode: InjectionMode) -> Result<(), InjectionError>;
    async fn send_enter(&self) -> Result<(), InjectionError>;
    fn backend_info(&self) -> InjectionBackendInfo;
}

pub fn create_injector() -> Arc<dyn TextInjector> {
    #[cfg(windows)]
    {
        return Arc::new(super::windows::WindowsInjector::new());
    }
    #[cfg(target_os = "macos")]
    {
        return Arc::new(super::macos::MacOsInjector::new());
    }
    #[cfg(target_os = "linux")]
    {
        if super::linux_wayland::is_wayland_session() {
            Arc::new(super::linux_wayland::WaylandInjector::new())
        } else {
            Arc::new(super::linux_x11::X11Injector::new())
        }
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Arc::new(UnavailableInjector)
    }
}

#[allow(dead_code)]
struct UnavailableInjector;

#[async_trait]
impl TextInjector for UnavailableInjector {
    async fn insert_text(&self, _text: &str, _mode: InjectionMode) -> Result<(), InjectionError> {
        Err(InjectionError::Unavailable)
    }

    async fn delete_backward(&self, _char_count: u32, _mode: InjectionMode) -> Result<(), InjectionError> {
        Err(InjectionError::Unavailable)
    }

    async fn send_enter(&self) -> Result<(), InjectionError> {
        Err(InjectionError::Unavailable)
    }

    fn backend_info(&self) -> InjectionBackendInfo {
        InjectionBackendInfo {
            available: false,
            backend: "unavailable".to_string(),
        }
    }
}

/// Mock injector for tests.
pub struct MockInjector {
    pub last_text: std::sync::Mutex<Option<String>>,
}

impl MockInjector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            last_text: std::sync::Mutex::new(None),
        })
    }
}

#[async_trait]
impl TextInjector for MockInjector {
    async fn insert_text(&self, text: &str, _mode: InjectionMode) -> Result<(), InjectionError> {
        *self.last_text.lock().unwrap() = Some(text.to_string());
        Ok(())
    }

    async fn delete_backward(&self, _char_count: u32, _mode: InjectionMode) -> Result<(), InjectionError> {
        Ok(())
    }

    async fn send_enter(&self) -> Result<(), InjectionError> {
        Ok(())
    }

    fn backend_info(&self) -> InjectionBackendInfo {
        InjectionBackendInfo {
            available: true,
            backend: "mock".to_string(),
        }
    }
}
