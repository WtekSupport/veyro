use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const MIC_HISTORY_LEN: usize = 40;

#[derive(Debug, Clone)]
pub struct MicMonitor {
    inner: Arc<Mutex<MicMonitorInner>>,
}

#[derive(Debug, Default)]
struct MicMonitorInner {
    history: VecDeque<u8>,
}

impl MicMonitor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MicMonitorInner::default())),
        }
    }

    pub fn push_level(&self, level_percent: u8) {
        if let Ok(mut guard) = self.inner.lock() {
            if guard.history.len() >= MIC_HISTORY_LEN {
                guard.history.pop_front();
            }
            guard.history.push_back(level_percent);
        }
    }

    pub fn history(&self) -> Vec<u8> {
        self.inner
            .lock()
            .map(|guard| guard.history.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.history.clear();
        }
    }
}

impl Default for MicMonitor {
    fn default() -> Self {
        Self::new()
    }
}
