pub mod clipboard;
pub mod focus_target;
pub mod focus_watch;
pub mod injector;
pub mod live;
pub mod prepare;
pub mod timing;
#[cfg(not(windows))]
pub mod keyboard_common;
#[cfg(windows)]
pub mod windows_keyboard;

#[cfg(target_os = "linux")]
pub mod linux_wayland;
#[cfg(target_os = "linux")]
pub mod linux_x11;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

pub use injector::{
    create_injector, InjectionBackendInfo, InjectionError, MockInjector, TextInjector,
};
