use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::warn;

use crate::injection::live::{insert_live_text, replace_live_text};
use crate::injection::{InjectionError, TextInjector};
use crate::settings::{AppSettings, InjectionMode};

const INDICATOR_FRAMES: [&str; 3] = [".", "..", "..."];
const INDICATOR_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Default)]
struct LiveDictationState {
    injected_text: String,
    anim_thread: Option<JoinHandle<()>>,
}

struct LiveDictationInner {
    active: AtomicBool,
    field_indicator: AtomicBool,
    state: Mutex<LiveDictationState>,
}

pub struct LiveDictationSession {
    inner: Arc<LiveDictationInner>,
}

impl Default for LiveDictationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveDictationSession {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(LiveDictationInner {
                active: AtomicBool::new(false),
                field_indicator: AtomicBool::new(false),
                state: Mutex::new(LiveDictationState::default()),
            }),
        }
    }

    pub fn start(&self, field_indicator: bool) {
        self.stop();
        self.inner.active.store(true, Ordering::SeqCst);
        self.inner
            .field_indicator
            .store(field_indicator, Ordering::SeqCst);

        if !field_indicator {
            return;
        }

        match insert_live_text(INDICATOR_FRAMES[0]) {
            Ok(()) => {
                if let Ok(mut state) = self.inner.state.lock() {
                    state.injected_text = INDICATOR_FRAMES[0].to_string();
                }
                self.spawn_animation_thread();
            }
            Err(error) => {
                warn!("live dictation indicator insert failed: {error}");
            }
        }
    }

    pub fn stop(&self) {
        self.inner.active.store(false, Ordering::SeqCst);
        self.join_animation_thread();
    }

    pub fn is_active(&self) -> bool {
        self.inner.active.load(Ordering::SeqCst)
    }

    pub fn has_injected(&self) -> bool {
        self.inner
            .state
            .lock()
            .ok()
            .is_some_and(|state| !state.injected_text.is_empty())
    }

    pub fn injected_text(&self) -> String {
        self.inner
            .state
            .lock()
            .ok()
            .map(|state| state.injected_text.clone())
            .unwrap_or_default()
    }

    pub fn reset(&self) {
        self.join_animation_thread();
        if let Ok(mut state) = self.inner.state.lock() {
            *state = LiveDictationState::default();
        }
    }

    pub async fn finalize(
        &self,
        final_text: &str,
        injector: Arc<dyn TextInjector>,
        settings: &AppSettings,
    ) -> Result<(), InjectionError> {
        let injected = self
            .inner
            .state
            .lock()
            .ok()
            .map(|state| state.injected_text.clone())
            .unwrap_or_default();

        self.stop();
        self.reset();

        if injected.is_empty() {
            return injector
                .insert_text(final_text, settings.injection_mode)
                .await;
        }

        let rollback_chars = char_count(&injected);
        if rollback_chars > 0 {
            delete_backward(injector.clone(), settings.injection_mode, rollback_chars).await?;
        }

        if !final_text.is_empty() {
            injector
                .insert_text(final_text, settings.injection_mode)
                .await?;
        }

        Ok(())
    }

    /// Remove animated "..." from the target field when recording ends without text.
    pub async fn clear_field_indicator(
        &self,
        injector: Arc<dyn TextInjector>,
        settings: &AppSettings,
    ) {
        if self.has_injected() {
            if let Err(error) = self.rollback(injector, settings).await {
                warn!("live dictation indicator cleanup failed: {error}");
            }
            return;
        }
        self.stop();
        self.reset();
    }

    pub async fn rollback(
        &self,
        injector: Arc<dyn TextInjector>,
        settings: &AppSettings,
    ) -> Result<(), InjectionError> {
        let injected = self
            .inner
            .state
            .lock()
            .ok()
            .map(|state| state.injected_text.clone())
            .unwrap_or_default();

        self.stop();
        self.reset();

        if injected.is_empty() {
            return Ok(());
        }

        delete_backward(injector, settings.injection_mode, char_count(&injected)).await
    }

    fn spawn_animation_thread(&self) {
        let inner = Arc::clone(&self.inner);
        let handle = thread::Builder::new()
            .name("live-dictation-indicator".into())
            .spawn(move || indicator_loop(inner))
            .ok();

        if let Ok(mut state) = self.inner.state.lock() {
            state.anim_thread = handle;
        }
    }

    fn join_animation_thread(&self) {
        let handle = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.anim_thread.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

fn indicator_loop(inner: Arc<LiveDictationInner>) {
    let mut frame_idx = 0usize;

    while inner.active.load(Ordering::SeqCst) {
        thread::sleep(INDICATOR_INTERVAL);
        if !inner.active.load(Ordering::SeqCst) || !inner.field_indicator.load(Ordering::SeqCst) {
            break;
        }

        frame_idx = (frame_idx + 1) % INDICATOR_FRAMES.len();
        let next = INDICATOR_FRAMES[frame_idx];
        let previous = inner
            .state
            .lock()
            .ok()
            .map(|state| state.injected_text.clone())
            .unwrap_or_default();
        if previous.is_empty() {
            continue;
        }

        let previous_chars = char_count(&previous);
        if let Ok(mut state) = inner.state.lock() {
            state.injected_text = next.to_string();
        }
        if replace_live_text(previous_chars, next).is_err() {
            if let Ok(mut state) = inner.state.lock() {
                state.injected_text = previous;
            }
        }
    }
}

fn char_count(text: &str) -> u32 {
    text.chars().count().min(u32::MAX as usize) as u32
}

async fn delete_backward(
    injector: Arc<dyn TextInjector>,
    mode: InjectionMode,
    char_count: u32,
) -> Result<(), InjectionError> {
    if char_count == 0 {
        return Ok(());
    }
    injector.delete_backward(char_count, mode).await
}
