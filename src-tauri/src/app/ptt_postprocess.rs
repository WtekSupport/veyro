use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::text::normalize::ensure_spaces_after_punctuation;

#[derive(Default)]
struct SessionState {
    active: bool,
    /// Exact text inserted in phase 1 (concatenated per segment, no extra separators).
    injected_text: String,
    press_enter: bool,
}

pub struct PttPostprocessSession {
    inner: Mutex<SessionState>,
    finish_in_progress: AtomicBool,
}

impl Default for PttPostprocessSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Avoid recording the same phrase twice when STT/VAD replays overlap.
fn split_non_duplicate_append(existing: &str, incoming: &str) -> Vec<String> {
    if incoming.is_empty() {
        return Vec::new();
    }

    let incoming_core = incoming.trim();
    if incoming_core.is_empty() {
        return Vec::new();
    }

    let existing_trim = existing.trim_end();
    if existing_trim.is_empty() {
        return vec![incoming.to_string()];
    }

    if existing.ends_with(incoming) || existing_trim.ends_with(incoming_core) {
        return Vec::new();
    }

    let incoming_words: Vec<_> = incoming_core.split_whitespace().collect();
    let existing_words: Vec<_> = existing_trim.split_whitespace().collect();
    if incoming_words.len() >= 8 && existing_words.len() >= incoming_words.len() {
        let window = incoming_words.len();
        if existing_words[existing_words.len() - window..] == incoming_words[..] {
            return Vec::new();
        }
    }

    vec![incoming.to_string()]
}

impl PttPostprocessSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SessionState::default()),
            finish_in_progress: AtomicBool::new(false),
        }
    }

    pub fn try_begin_finish(&self) -> bool {
        self.finish_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn finish_finish(&self) {
        self.finish_in_progress.store(false, Ordering::SeqCst);
    }

    pub fn begin_session(&self) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        *state = SessionState {
            active: true,
            ..Default::default()
        };
    }

    pub fn reset(&self) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        *state = SessionState::default();
    }

    pub fn is_active(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|state| state.active)
    }

    pub fn has_injected_text(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|state| !state.injected_text.is_empty())
    }

    pub fn press_enter(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|state| state.press_enter)
    }

    pub fn record_phase1_injection(&self, text: &str, press_enter: bool) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if !state.active {
            return;
        }
        if text.is_empty() {
            if press_enter {
                state.press_enter = true;
            }
            return;
        }

        for fragment in split_non_duplicate_append(&state.injected_text, text) {
            state.injected_text.push_str(&fragment);
        }
        state.injected_text = ensure_spaces_after_punctuation(&state.injected_text);
        if press_enter {
            state.press_enter = true;
        }
    }

    pub fn take_finish_snapshot(&self) -> Option<(String, bool)> {
        let Ok(mut state) = self.inner.lock() else {
            return None;
        };
        if !state.active || state.injected_text.is_empty() {
            state.active = false;
            return None;
        }

        let snapshot = (
            std::mem::take(&mut state.injected_text),
            state.press_enter,
        );
        state.press_enter = false;
        state.active = false;
        Some(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_injected_text_for_single_rewrite() {
        let session = PttPostprocessSession::new();
        session.begin_session();
        session.record_phase1_injection("Hello world ", false);
        session.record_phase1_injection("Second phrase ", false);

        assert_eq!(session.inner.lock().unwrap().injected_text, "Hello world Second phrase ");

        let (text, enter) = session.take_finish_snapshot().unwrap();
        assert_eq!(text, "Hello world Second phrase ");
        assert!(!enter);
        assert!(!session.is_active());
    }

    #[test]
    fn skips_duplicate_append_for_session_buffer() {
        let session = PttPostprocessSession::new();
        session.begin_session();
        session.record_phase1_injection("Hello world ", false);
        session.record_phase1_injection("Hello world ", false);
        assert_eq!(session.inner.lock().unwrap().injected_text, "Hello world ");
    }
}
