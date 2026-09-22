use std::sync::atomic::{AtomicU64, Ordering};

/// Tracks dictation session ids so aborted sessions skip queued pipeline work.
pub struct DictationSession {
    current_id: AtomicU64,
    aborted_id: AtomicU64,
    /// Successful text injections in the active session (multi-segment / continuous).
    injection_count: AtomicU64,
}

impl Default for DictationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl DictationSession {
    pub fn new() -> Self {
        Self {
            current_id: AtomicU64::new(0),
            aborted_id: AtomicU64::new(0),
            injection_count: AtomicU64::new(0),
        }
    }

    pub fn current_id(&self) -> u64 {
        self.current_id.load(Ordering::SeqCst)
    }

    pub fn begin_session(&self) -> u64 {
        self.injection_count.store(0, Ordering::SeqCst);
        self.current_id.fetch_add(1, Ordering::SeqCst).saturating_add(1)
    }

    pub fn injection_count(&self) -> u64 {
        self.injection_count.load(Ordering::SeqCst)
    }

    pub fn record_injection(&self) {
        self.injection_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Marks the active session aborted. Returns the session id if one was active.
    pub fn abort_current(&self) -> Option<u64> {
        let current = self.current_id.load(Ordering::SeqCst);
        if current == 0 {
            return None;
        }
        let _ = self
            .aborted_id
            .fetch_max(current, Ordering::SeqCst);
        Some(current)
    }

    pub fn is_session_aborted(&self, session_id: u64) -> bool {
        if session_id == 0 {
            return false;
        }
        session_id <= self.aborted_id.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_increments_and_clears_abort() {
        let session = DictationSession::new();
        let a = session.begin_session();
        assert_eq!(a, 1);
        assert!(session.abort_current().is_some());
        assert!(session.is_session_aborted(1));
        let b = session.begin_session();
        assert_eq!(b, 2);
        assert!(!session.is_session_aborted(2));
        assert!(session.is_session_aborted(1));
    }

    #[test]
    fn abort_without_session_returns_none() {
        let session = DictationSession::new();
        assert!(session.abort_current().is_none());
    }
}
