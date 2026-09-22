use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
#[cfg(windows)]
use std::thread::{self};
use std::time::Duration;

#[cfg(windows)]
use super::focus_target;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

struct WatchState {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

static WATCH: Mutex<Option<WatchState>> = Mutex::new(None);

/// Poll foreground/focus vs captured injection target.
#[cfg(windows)]
pub fn start_focus_watch_abort(session_id: u64, on_lost: Box<dyn FnOnce() + Send>) {
    stop_focus_watch();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);
    let fired = Arc::new(AtomicU64::new(0));

    let mut on_lost = Some(on_lost);
    let handle = thread::spawn(move || {
        while !stop_worker.load(Ordering::SeqCst) {
            thread::sleep(POLL_INTERVAL);
            if stop_worker.load(Ordering::SeqCst) {
                break;
            }
            if fired.load(Ordering::SeqCst) == session_id {
                continue;
            }
            if focus_target::focus_target_matches() {
                continue;
            }
            if fired
                .compare_exchange(0, session_id, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                continue;
            }
            if let Some(callback) = on_lost.take() {
                callback();
            }
            break;
        }
    });

    if let Ok(mut guard) = WATCH.lock() {
        *guard = Some(WatchState {
            stop,
            handle: Some(handle),
        });
    }
}

/// Continuous poll: `on_lost` / `on_regained` on focus transitions until `stop_focus_watch`.
#[cfg(windows)]
pub fn start_focus_watch_defer(
    on_lost: Box<dyn Fn() + Send + 'static>,
    on_regained: Box<dyn Fn() + Send + 'static>,
) {
    stop_focus_watch();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let mut was_matching = focus_target::focus_target_matches();
        while !stop_worker.load(Ordering::SeqCst) {
            thread::sleep(POLL_INTERVAL);
            if stop_worker.load(Ordering::SeqCst) {
                break;
            }
            let matching = focus_target::focus_target_matches();
            if was_matching && !matching {
                on_lost();
            } else if !was_matching && matching {
                on_regained();
            }
            was_matching = matching;
        }
    });

    if let Ok(mut guard) = WATCH.lock() {
        *guard = Some(WatchState {
            stop,
            handle: Some(handle),
        });
    }
}

#[cfg(not(windows))]
pub fn start_focus_watch_abort(_session_id: u64, _on_lost: Box<dyn FnOnce() + Send>) {}

#[cfg(not(windows))]
pub fn start_focus_watch_defer(
    _on_lost: Box<dyn Fn() + Send + 'static>,
    _on_regained: Box<dyn Fn() + Send + 'static>,
) {}

/// Legacy entry: abort-on-loss one-shot (kept for call-site clarity).
#[cfg(windows)]
pub fn start_focus_watch(session_id: u64, on_lost: Box<dyn FnOnce() + Send>) {
    start_focus_watch_abort(session_id, on_lost);
}

#[cfg(not(windows))]
pub fn start_focus_watch(_session_id: u64, _on_lost: Box<dyn FnOnce() + Send>) {}

pub fn stop_focus_watch() {
    let state = WATCH.lock().ok().and_then(|mut guard| guard.take());
    if let Some(state) = state {
        state.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = state.handle {
            let _ = handle.join();
        }
    }
}
