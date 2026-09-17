use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender};
use tauri::AppHandle;
use tracing::warn;

use crate::error::AppError;
use crate::hotkey::binding::KeyBinding;
use crate::hotkey::runtime::{process_key_event, KeyEvent};

pub struct HookHandle {
    pub hook_thread: JoinHandle<()>,
    pub worker_thread: JoinHandle<()>,
}

pub fn spawn_worker(
    rx: Receiver<KeyEvent>,
    app: AppHandle,
    binding: KeyBinding,
) -> Result<JoinHandle<()>, AppError> {
    thread::Builder::new()
        .name("veyro-hotkey-worker".into())
        .spawn(move || {
            let mut mods = crate::hotkey::binding::ModifierState::default();
            let mut combo_active = false;
            while let Ok(event) = rx.recv() {
                process_key_event(&binding, &mut mods, &mut combo_active, &app, event);
            }
        })
        .map_err(|error| AppError::Hotkey(format!("hotkey worker thread failed: {error}")))
}

pub fn join_hook(handle: HookHandle) {
    if handle.hook_thread.join().is_err() {
        warn!("low-level hotkey hook thread panicked");
    }
    if handle.worker_thread.join().is_err() {
        warn!("low-level hotkey worker thread panicked");
    }
}

pub type EventSender = Sender<KeyEvent>;
