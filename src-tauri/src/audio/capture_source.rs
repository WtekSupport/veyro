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

    /// Whether two sources match for dedicated handoff.
    pub fn capture_format(&self) -> Result<(u32, u16), crate::error::AudioError> {
        match self {
            Self::Microphone { device_id } => {
                crate::audio::warmup::query_device_format(device_id.as_deref())
            }
            Self::MicrophoneAndLoopback { .. } => Ok((16_000, 1)),
        }
    }

    pub fn same_effective_capture(&self, other: &CaptureSource) -> bool {
        match (self, other) {
            (
                Self::Microphone { device_id: left },
                Self::Microphone { device_id: right },
            ) => left == right,
            (
                Self::MicrophoneAndLoopback {
                    device_id: left_mic,
                    process_id: left_pid,
                    ..
                },
                Self::MicrophoneAndLoopback {
                    device_id: right_mic,
                    process_id: right_pid,
                    ..
                },
            ) => left_mic == right_mic && left_pid == right_pid,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_effective_capture_matches_mic_device() {
        let left = CaptureSource::Microphone {
            device_id: Some("id".into()),
        };
        let right = CaptureSource::Microphone {
            device_id: Some("id".into()),
        };
        assert!(left.same_effective_capture(&right));
    }

    #[test]
    fn same_effective_capture_rejects_mic_vs_loopback() {
        let mic = CaptureSource::Microphone { device_id: None };
        let loopback = CaptureSource::MicrophoneAndLoopback {
            device_id: None,
            process_id: 1,
            label: "x".into(),
        };
        assert!(!mic.same_effective_capture(&loopback));
    }
}
