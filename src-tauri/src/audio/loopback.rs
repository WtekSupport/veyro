//! Application output (loopback) capture — Windows process loopback via WASAPI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopbackAppInfo {
    pub process_id: u32,
    pub name: String,
    pub peak: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopbackAppsResponse {
    pub supported: bool,
    pub apps: Vec<LoopbackAppInfo>,
}

#[cfg(windows)]
mod windows_impl;

#[cfg(windows)]
pub use windows_impl::{list_loopback_apps, start_app_loopback_stream, LoopbackStreamHandle};

#[cfg(not(windows))]
pub struct LoopbackStreamHandle;

#[cfg(not(windows))]
impl LoopbackStreamHandle {
    pub fn stop(&self) {}
}

#[cfg(not(windows))]
pub fn list_loopback_apps() -> LoopbackAppsResponse {
    LoopbackAppsResponse {
        supported: false,
        apps: Vec::new(),
    }
}

#[cfg(not(windows))]
pub fn start_app_loopback_stream(
    _process_id: u32,
    _label: &str,
    _sample_tx: crossbeam_channel::Sender<Vec<f32>>,
    _mic_level: std::sync::Arc<std::sync::atomic::AtomicU32>,
    _mic_monitor: Option<crate::audio::monitor::MicMonitor>,
) -> Result<(LoopbackStreamHandle, u32, u16, String), crate::error::AudioError> {
    Err(crate::error::AudioError::Stream(
        "app loopback capture is only available on Windows".into(),
    ))
}
