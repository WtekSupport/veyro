use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::AudioError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());

    let devices = host
        .input_devices()
        .map_err(|_| AudioError::MicrophoneUnavailable)?;

    let mut result = Vec::new();
    for (index, device) in devices.enumerate() {
        let name = device.name().unwrap_or_else(|_| format!("Microphone {index}"));
        let is_default = default_name.as_deref() == Some(name.as_str());
        result.push(AudioDeviceInfo {
            id: name.clone(),
            name,
            is_default,
        });
    }

    if result.is_empty() {
        return Err(AudioError::MicrophoneUnavailable);
    }

    Ok(result)
}

pub fn resolve_input_device(device_id: Option<&str>) -> Result<cpal::Device, AudioError> {
    let host = cpal::default_host();

    if let Some(id) = device_id.filter(|value| !value.is_empty()) {
        let devices = host
            .input_devices()
            .map_err(|_| AudioError::MicrophoneUnavailable)?;
        for device in devices {
            if device.name().ok().as_deref() == Some(id) {
                return Ok(device);
            }
        }
        warn!("saved microphone '{id}' not found, using system default");
    }

    host.default_input_device()
        .ok_or(AudioError::MicrophoneUnavailable)
}
