use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_ENTRIES: usize = 120;
const MAX_DISK_LOG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLevel {
    Info,
    Warn,
    Error,
}

impl ActivityLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub timestamp_ms: u64,
    pub level: String,
    pub message_key: String,
    #[serde(default)]
    pub message_args: Value,
}

pub struct ActivityLog {
    entries: Mutex<VecDeque<ActivityLogEntry>>,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityLog {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)),
        }
    }

    pub fn push(
        &self,
        level: ActivityLevel,
        message_key: impl Into<String>,
        message_args: Value,
    ) -> ActivityLogEntry {
        let entry = ActivityLogEntry {
            timestamp_ms: now_ms(),
            level: level.as_str().to_string(),
            message_key: message_key.into(),
            message_args,
        };

        let mut entries = self.entries.lock().expect("activity log lock poisoned");
        if entries.len() >= MAX_ENTRIES {
            entries.pop_front();
        }
        entries.push_back(entry.clone());
        persist_entry(&entry);
        entry
    }

    pub fn snapshot(&self) -> Vec<ActivityLogEntry> {
        self.entries
            .lock()
            .expect("activity log lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .expect("activity log lock poisoned")
            .clear();
        if let Some(path) = activity_log_path() {
            let _ = fs::write(&path, "");
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn activity_log_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Veyro").join("debug").join("activity.log"))
}

fn truncate_disk_log_if_needed(path: &PathBuf) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() <= MAX_DISK_LOG_BYTES {
        return;
    }
    let _ = fs::write(path, "");
}

fn persist_entry(entry: &ActivityLogEntry) {
    let Some(path) = activity_log_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    truncate_disk_log_if_needed(&path);
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let args = entry.message_args.to_string();
    let _ = writeln!(
        file,
        "{} {} {} {}",
        entry.timestamp_ms, entry.level, entry.message_key, args
    );
}
