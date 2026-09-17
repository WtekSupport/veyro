#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod common;

use tauri::AppHandle;

use crate::error::AppError;

#[cfg(target_os = "macos")]
pub fn register(app: &AppHandle, hotkey: &str) -> Result<(), AppError> {
    macos::register(app, hotkey)
}

#[cfg(target_os = "linux")]
pub fn register(app: &AppHandle, hotkey: &str) -> Result<(), AppError> {
    linux::register(app, hotkey)
}

#[cfg(target_os = "macos")]
pub fn unregister() {
    macos::unregister();
}

#[cfg(target_os = "linux")]
pub fn unregister() {
    linux::unregister();
}
