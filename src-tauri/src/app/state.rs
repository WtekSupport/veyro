use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    Initializing,
    Disabled,
    Ready,
    Listening,
    Processing,
    Transcribing,
    Injecting,
    Error,
    PermissionRequired,
    MicrophoneUnavailable,
    NetworkUnavailable,
}

impl AppState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Initializing => "INITIALIZING",
            Self::Disabled => "DISABLED",
            Self::Ready => "READY",
            Self::Listening => "LISTENING",
            Self::Processing => "PROCESSING",
            Self::Transcribing => "TRANSCRIBING",
            Self::Injecting => "INJECTING",
            Self::Error => "ERROR",
            Self::PermissionRequired => "PERMISSION REQUIRED",
            Self::MicrophoneUnavailable => "MICROPHONE UNAVAILABLE",
            Self::NetworkUnavailable => "NETWORK UNAVAILABLE",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use AppState::*;

        if self == next {
            return true;
        }

        matches!(
            (self, next),
            (Initializing, Disabled)
                | (Initializing, Ready)
                | (Initializing, PermissionRequired)
                | (Initializing, MicrophoneUnavailable)
                | (Disabled, Initializing)
                | (Ready, Disabled)
                | (Ready, Listening)
                | (Ready, Processing)
                | (Ready, Error)
                | (Ready, MicrophoneUnavailable)
                | (Listening, Processing)
                | (Listening, Ready)
                | (Listening, Disabled)
                | (Listening, Error)
                | (Processing, Transcribing)
                | (Processing, Injecting)
                | (Processing, Ready)
                | (Processing, Error)
                | (Processing, NetworkUnavailable)
                | (Transcribing, Injecting)
                | (Transcribing, Ready)
                | (Transcribing, Error)
                | (Transcribing, NetworkUnavailable)
                | (Injecting, Ready)
                | (Injecting, Error)
                | (Error, Ready)
                | (Error, Disabled)
                | (PermissionRequired, Disabled)
                | (PermissionRequired, Ready)
                | (MicrophoneUnavailable, Disabled)
                | (MicrophoneUnavailable, Ready)
                | (NetworkUnavailable, Ready)
                | (NetworkUnavailable, Disabled)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub state: AppState,
    pub enabled: bool,
    pub push_to_talk: bool,
    pub ptt_hold: bool,
    pub hotkey: String,
    pub microphone_device: Option<String>,
    pub language: Option<String>,
    pub injection_mode: crate::settings::InjectionMode,
    pub last_error: Option<String>,
    pub injection_available: bool,
    pub injection_backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub state: AppState,
    pub audio_running: bool,
    pub audio_capturing: bool,
    pub push_to_talk: bool,
    pub hotkey: String,
    pub injection_available: bool,
    pub injection_backend: String,
    pub has_api_key: bool,
    pub microphone_device: Option<String>,
    pub pending_segments: usize,
    pub callbacks_enabled: bool,
    pub whisper_backend: String,
    pub whisper_local_compiled: bool,
    pub whisper_gpu_compiled: bool,
    pub capslock_ptt_supported: bool,
    pub hotkey_game_mode_supported: bool,
    pub hotkey_backend: String,
    pub hook_active: bool,
    pub text_processing_mode: String,
    pub transcription_provider: String,
    pub text_rewrite_provider: String,
    pub local_llm_model: String,
    pub local_llm_compiled: bool,
    pub local_llm_gpu_compiled: bool,
    pub local_llm_ready: bool,
    pub whisper_loaded: bool,
    #[serde(default)]
    pub local_stt_loaded: bool,
    #[serde(default)]
    pub local_stt_engine: String,
    #[serde(default)]
    pub local_stt_model: String,
    #[serde(default)]
    pub sherpa_stt_compiled: bool,
    pub llm_loaded: bool,
    pub settings_webview_alive: bool,
    pub about_webview_alive: bool,
    pub process_elevated: bool,
    pub hotkeys_blocked_by_elevation: bool,
    pub show_notifications: bool,
    pub system_notifications_available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_can_enter_listening() {
        assert!(AppState::Ready.can_transition_to(AppState::Listening));
    }

    #[test]
    fn disabled_cannot_jump_to_processing() {
        assert!(!AppState::Disabled.can_transition_to(AppState::Processing));
    }

    #[test]
    fn error_can_recover_to_ready() {
        assert!(AppState::Error.can_transition_to(AppState::Ready));
    }
}
