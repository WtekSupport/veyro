use std::sync::Mutex;

use crate::text::basic_cleanup::{polish_continuation_injection, trailing_text_ends_sentence};
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

    /// Drop all buffered text. Only for abort/cancel or idle retarget onto a new field.
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

    /// Whether buffered text currently ends a sentence (for cross-chunk capital polish).
    pub fn trailing_ends_sentence(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .map(|inner| trailing_text_ends_sentence(&inner.text))
            .unwrap_or(true)
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
            let previous_ends = trailing_text_ends_sentence(&inner.text);
            normalized = polish_continuation_injection(&normalized, previous_ends);
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

    /// Clone buffered text for injection without clearing (consume only after success).
    pub fn snapshot_for_flush(&self) -> Option<(String, bool)> {
        let Ok(inner) = self.inner.lock() else {
            return None;
        };
        if inner.text.is_empty() {
            return None;
        }
        Some((inner.text.clone(), inner.pending_enter))
    }

    /// Remove a previously snapshotted prefix after successful injection.
    ///
    /// If more text was appended during the flush, only the flushed prefix is dropped.
    pub fn consume_flushed_snapshot(&self, flushed: &str, flushed_enter: bool) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if flushed.is_empty() {
            return;
        }
        if inner.text.starts_with(flushed) {
            inner.text = inner.text[flushed.len()..].to_string();
        } else if inner.text == flushed {
            inner.text.clear();
        } else {
            // Snapshot stale vs concurrent mutation — keep buffer intact.
            return;
        }

        if inner.text.is_empty() {
            *inner = Inner::default();
            return;
        }

        // Enter applied with the flushed chunk; remainder keeps its own flag if set later.
        if flushed_enter {
            inner.pending_enter = false;
        }
        // segment_count is approximate after partial consume; clamp to at least 1 while non-empty.
        if inner.segment_count == 0 {
            inner.segment_count = 1;
        }
    }
}

/// Whether retargeting the injection HWND may drop the defer buffer.
///
/// Abort-off + in-flight dictation (retain) must never discard accumulated text.
pub(crate) fn should_clear_buffer_on_retarget(
    abort_on_focus_loss: bool,
    session_in_flight: bool,
) -> bool {
    if abort_on_focus_loss {
        return true;
    }
    !session_in_flight
}

/// Keep the same dictation session across VAD Ready gaps while abort-off and away from the field
/// **only while deferred text or jobs are still in flight**.
///
/// Idle Ready + unfocused must NOT continue: that re-bound the next utterance to the old
/// sticky HWND after the user switched fields.
pub(crate) fn should_continue_session_across_focus_loss_state(
    abort_on_focus_loss: bool,
    session_id: u64,
    target_hwnd: isize,
    focus_matches: bool,
    has_pending_buffer: bool,
    pending_jobs: usize,
) -> bool {
    if abort_on_focus_loss || session_id == 0 || target_hwnd == 0 {
        return false;
    }
    if has_pending_buffer || pending_jobs > 0 {
        return true;
    }
    let _ = focus_matches;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_polishes_second_fragment() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("Hello. ".to_string(), false, 0);
        buffer.append_segment("world".to_string(), false, 0);
        let (text, enter) = buffer.snapshot_for_flush().unwrap();
        assert!(!enter);
        assert!(text.contains("Hello."));
        assert!(text.contains("World"), "out: {text}");
        buffer.consume_flushed_snapshot(&text, enter);
        assert!(buffer.is_empty());
    }

    #[test]
    fn append_keeps_lowercase_when_previous_mid_sentence() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("Hello ".to_string(), false, 0);
        buffer.append_segment("World".to_string(), false, 0);
        let (text, enter) = buffer.snapshot_for_flush().unwrap();
        assert!(text.contains("world"), "out: {text}");
        buffer.consume_flushed_snapshot(&text, enter);
        assert!(buffer.is_empty());
    }

    #[test]
    fn pending_enter_survives_until_consume() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("Hi".to_string(), true, 0);
        let (text, enter) = buffer.snapshot_for_flush().unwrap();
        assert!(enter);
        assert!(!buffer.is_empty());
        buffer.consume_flushed_snapshot(&text, enter);
        assert!(buffer.snapshot_for_flush().is_none());
    }

    #[test]
    fn clear_resets_state() {
        let buffer = FocusDeferBuffer::new();
        buffer.append_segment("x".to_string(), true, 0);
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.segment_count(), 0);
    }

    #[test]
    fn snapshot_keeps_text_until_consume() {
        let buffer = FocusDeferBuffer::new();
        buffer.push_prepared("alpha ".to_string(), false);
        let (snap, enter) = buffer.snapshot_for_flush().unwrap();
        assert_eq!(snap, "alpha ");
        assert!(!enter);
        assert!(buffer.has_pending());
        // Simulated failed inject: do not consume.
        assert!(buffer.has_pending());
        buffer.consume_flushed_snapshot(&snap, enter);
        assert!(buffer.is_empty());
    }

    #[test]
    fn consume_keeps_text_appended_during_flush() {
        let buffer = FocusDeferBuffer::new();
        buffer.push_prepared("one ".to_string(), false);
        let (snap, enter) = buffer.snapshot_for_flush().unwrap();
        buffer.push_prepared("two ".to_string(), false);
        buffer.consume_flushed_snapshot(&snap, enter);
        let (rest, _) = buffer.snapshot_for_flush().unwrap();
        assert_eq!(rest, "two ");
    }

    #[test]
    fn retarget_clear_policy() {
        assert!(should_clear_buffer_on_retarget(true, true));
        assert!(should_clear_buffer_on_retarget(true, false));
        assert!(!should_clear_buffer_on_retarget(false, true));
        assert!(should_clear_buffer_on_retarget(false, false));
    }

    #[test]
    fn continue_session_while_unfocused_abort_off() {
        // In-flight buffer/jobs: keep session while away.
        assert!(should_continue_session_across_focus_loss_state(
            false, 3, 42, false, true, 0
        ));
        assert!(should_continue_session_across_focus_loss_state(
            false, 3, 42, true, true, 0
        ));
        assert!(should_continue_session_across_focus_loss_state(
            false, 3, 42, true, false, 2
        ));
        // Idle Ready + unfocused: start a fresh session (new field may be focused).
        assert!(!should_continue_session_across_focus_loss_state(
            false, 3, 42, false, false, 0
        ));
    }

    #[test]
    fn do_not_continue_session_when_abort_on_or_idle_focused() {
        assert!(!should_continue_session_across_focus_loss_state(
            true, 3, 42, false, false, 0
        ));
        assert!(!should_continue_session_across_focus_loss_state(
            false, 0, 42, false, false, 0
        ));
        assert!(!should_continue_session_across_focus_loss_state(
            false, 3, 0, false, false, 0
        ));
        assert!(!should_continue_session_across_focus_loss_state(
            false, 3, 42, true, false, 0
        ));
    }
}
