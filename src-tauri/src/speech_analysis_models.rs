use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInstallStatus {
    NotInstalled,
    Downloading,
    Verifying,
    Installed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct SessionStatusRecord {
    pub status: ModelInstallStatus,
    pub failed_message_key: Option<String>,
}

#[derive(Default)]
pub struct SpeechAnalysisModelSession {
    inner: Mutex<HashMap<String, SessionStatusRecord>>,
}

impl SpeechAnalysisModelSession {
    pub fn snapshot(&self) -> HashMap<String, SessionStatusRecord> {
        self.inner
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn set_status(&self, variant_id: &str, status: ModelInstallStatus) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                variant_id.to_string(),
                SessionStatusRecord {
                    status,
                    failed_message_key: None,
                },
            );
        }
    }

    pub fn set_failed(&self, variant_id: &str, message_key: String) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                variant_id.to_string(),
                SessionStatusRecord {
                    status: ModelInstallStatus::Failed,
                    failed_message_key: Some(message_key),
                },
            );
        }
    }

    pub fn clear_transient(&self, variant_id: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.remove(variant_id);
        }
    }
}
