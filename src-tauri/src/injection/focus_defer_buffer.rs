use std::sync::Mutex;

use crate::text::basic_cleanup::polish_continuation_injection;
use crate::text::normalize::{ensure_spaces_after_punctuation, ensure_trailing_block_separator};

#[derive(Default)]
struct Inner {
    text: String,
    pending_enter: bool,
    /// Buffered segments in this defer period (for continuation polish with direct injects).
    segment_count: u64,
}

pub struct FocusDeferBuffer {
    inner: Mutex<Inner>,
}

impl Default for FocusDeferBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusDeferBuffer {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            *inner = Inner::default();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_none_or(|inner| inner.text.is_empty())
    }

    pub fn has_pending(&self) -> bool {
        !self.is_empty()
    }

    pub fn segment_count(&self) -> u64 {
        self.inner
            .lock()
            .ok()
            .map(|inner| inner.segment_count)
            .unwrap_or(0)
    }

    /// Append a normalized segment while focus is away from the injection target.
    pub fn append_segment(
        &self,
        mut normalized: String,
        press_enter: bool,
        prior_output_count: u64,
    ) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if normalized.is_empty() {
            if press_enter {
                inner.pending_enter = true;
            }
            return;
        }

        normalized = ensure_spaces_after_punctuation(&normalized);
        let continuation = prior_output_count.saturating_add(inner.segment_count);
        if continuation > 0 {
            normalized = polish_continuation_injection(&normalized);
        }
        normalized = ensure_trailing_block_separator(&normalized);

        inner.text.push_str(&normalized);
        inner.segment_count = inner.segment_count.saturating_add(1);
        if press_enter {
            inner.pending_enter = true;
        }
    }

    /// Append text already normalized/polished by the pipeline (defer path).
    pub fn push_prepared(&self, text: String, press_enter: bool) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if text.is_empty() {
            if press_enter {
                inner.pending_enter = true;
            }
            return;
        }
        inner.text.push_str(&text);
        inner.segment_count = inner.segment_count.saturating_add(1);
        if press_enter {
            inner.pending_enter = true;
        }
    }

    /// Take buffered text for a single flush injection; clears buffer state.
    pub fn take_for_flush(&self) -> Option<(String, bool)> {
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        if inner.text.is_empty() {
            inner.pending_enter = false;
            return None;
        }
        let text = std::mem::take(&mut inner.text);
        let press_enter = inner.pending_enter;
        inner.pending_enter = false;
        Some((text, press_enter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_polishes_second_fragment() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("Hello. ".to_string(), false, 0);
        buffer.append_segment("world".to_string(), false, 0);
        let (text, enter) = buffer.take_for_flush().unwrap();
        assert!(!enter);
        assert!(text.contains("Hello."));
        assert!(text.contains("world"));
        assert!(buffer.is_empty());
    }

    #[test]
    fn pending_enter_survives_until_flush() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("Hi".to_string(), true, 0);
        let (_, enter) = buffer.take_for_flush().unwrap();
        assert!(enter);
        assert!(buffer.take_for_flush().is_none());
    }

    #[test]
    fn clear_resets_state() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("x".to_string(), true, 0);
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.segment_count(), 0);
    }
}
