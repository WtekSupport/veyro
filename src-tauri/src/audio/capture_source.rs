use serde::{Deserialize, Serialize};

use crate::settings::AppSettings;

/// Where live dictation samples come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSourceKind {
    #[default]
    Microphone,
    /// Microphone plus Windows WASAPI process loopback (mixed).
    #[serde(alias = "app_loopback")]
    MicrophoneAndLoopback,
}

impl CaptureSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::MicrophoneAndLoopback => "microphone_and_loopback",
        }
    }

    pub fn includes_loopback(self) -> bool {
        matches!(self, Self::MicrophoneAndLoopback)
    }
}

/// Runtime capture endpoint resolved from settings.
#[derive(Debug, Clone)]
pub enum CaptureSource {
    Microphone {
        device_id: Option<String>,
    },
    /// Mic + app output mixed to mono 16 kHz for VAD/STT.
    MicrophoneAndLoopback {
        device_id: Option<String>,
        process_id: u32,
        label: String,
    },
}

impl CaptureSource {
    pub fn from_settings(settings: &AppSettings) -> Self {
        match settings.capture_source {
            CaptureSourceKind::MicrophoneAndLoopback => {
                if let Some(pid) = settings.loopback_app_pid.filter(|pid| *pid > 0) {
                    let label = settings
                        .loopback_app_name
                        .clone()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| format!("PID {pid}"));
                    return Self::MicrophoneAndLoopback {
                        device_id: settings.microphone_device.clone(),
                        process_id: pid,
                        label,
                    };
                }
                Self::Microphone {
                    device_id: settings.microphone_device.clone(),
                }
            }
            CaptureSourceKind::Microphone => Self::Microphone {
                device_id: settings.microphone_device.clone(),
            },
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Microphone { device_id } => device_id
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            Self::MicrophoneAndLoopback {
                device_id, label, ..
            } => {
                let mic = device_id
                    .as_deref()
                    .unwrap_or("default");
                format!("{mic} + {label}")
            }
        }
    }

    pub fn includes_loopback(&self) -> bool {
        matches!(self, Self::MicrophoneAndLoopback { .. })
    }
}
