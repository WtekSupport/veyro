use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Tracks dictation session ids so aborted sessions skip queued pipeline work.
pub struct DictationSession {
    current_id: AtomicU64,
    aborted_id: AtomicU64,
    /// Successful text injections in the active session (multi-segment / continuous).
    injection_count: AtomicU64,
    /// Whether the last injected/normalized segment ended a sentence (chunk capital polish).
    last_injection_ends_sentence: AtomicBool,
    /// Last Whisper language hint in auto mode (post-process / prompt context only; not STT lock).
    stt_language: Mutex<Option<String>>,
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
            last_injection_ends_sentence: AtomicBool::new(false),
            stt_language: Mutex::new(None),
        }
    }

    pub fn current_id(&self) -> u64 {
        self.current_id.load(Ordering::SeqCst)
    }

    pub fn begin_session(&self) -> u64 {
        self.injection_count.store(0, Ordering::SeqCst);
        self.last_injection_ends_sentence
            .store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.stt_language.lock() {
            *guard = None;
        }
        self.current_id.fetch_add(1, Ordering::SeqCst).saturating_add(1)
    }

    pub fn locked_stt_language(&self) -> Option<String> {
        self.stt_language
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Remember first reliable Whisper detection in auto mode for post-process hints.
    pub fn note_stt_language_from_segment(
        &self,
        fixed_language: Option<&str>,
        detected: Option<&str>,
        has_text: bool,
    ) {
        if fixed_language.is_some() || !has_text {
            return;
        }
        let Some(code) = detected.map(crate::transcription::normalize_stt_language_code) else {
            return;
        };
        if let Ok(mut guard) = self.stt_language.lock() {
            if guard.is_none() {
                *guard = Some(code);
            }
        }
    }

    pub fn injection_count(&self) -> u64 {
        self.injection_count.load(Ordering::SeqCst)
    }

    pub fn last_injection_ends_sentence(&self) -> bool {
        self.last_injection_ends_sentence.load(Ordering::SeqCst)
    }

    pub fn record_injection(&self) {
        self.record_injection_with_ending(false);
    }

    pub fn record_injection_with_ending(&self, ends_sentence: bool) {
        self.injection_count.fetch_add(1, Ordering::SeqCst);
        self.last_injection_ends_sentence
            .store(ends_sentence, Ordering::SeqCst);
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

    #[test]
    fn stt_language_resets_on_new_session_and_sticks_when_auto() {
        let session = DictationSession::new();
        let _ = session.begin_session();
        session.note_stt_language_from_segment(None, Some("en"), true);
        assert_eq!(session.locked_stt_language().as_deref(), Some("en"));
        session.note_stt_language_from_segment(None, Some("ru"), true);
        assert_eq!(session.locked_stt_language().as_deref(), Some("en"));
        let _ = session.begin_session();
        assert!(session.locked_stt_language().is_none());
    }
}
