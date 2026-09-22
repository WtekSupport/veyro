use std::sync::OnceLock;
use std::thread;

use crossbeam_channel::{Receiver, Sender, TrySendError, unbounded};
use tauri::AppHandle;
use tracing::{info, warn};

use crate::game_input::PttSignal;
use crate::hotkey::manager;

static SIGNAL_TX: OnceLock<Sender<PttSignal>> = OnceLock::new();

pub fn ensure_running(app: AppHandle) {
    if SIGNAL_TX.get().is_some() {
        return;
    }

    let (tx, rx) = unbounded();
    let _ = SIGNAL_TX.set(tx);

    thread::Builder::new()
        .name("veyro-ptt-gate".into())
        .spawn(move || gate_loop(app, rx))
        .map_err(|error| warn!("ptt gate thread failed: {error}"))
        .ok();
}

pub fn try_send_signal(signal: PttSignal) {
    let Some(tx) = SIGNAL_TX.get() else {
        warn!(?signal, "ptt signal dropped (gate not running)");
        return;
    };
    info!(?signal, "ptt signal queued");
    if let Err(TrySendError::Disconnected(_)) = tx.try_send(signal) {
        warn!(?signal, "ptt signal channel disconnected");
    }
}

pub fn dispatch_signal(app: &AppHandle, signal: PttSignal) {
    info!(?signal, "ptt signal dispatch");
    match signal {
        PttSignal::Pressed => manager::dispatch_ptt_pressed(app),
        PttSignal::Released => manager::dispatch_ptt_released(app),
        PttSignal::ToggleStop => manager::dispatch_ptt_toggle_stop(app),
    }
}

fn gate_loop(app: AppHandle, rx: Receiver<PttSignal>) {
    while let Ok(signal) = rx.recv() {
        dispatch_signal(&app, signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_channel_accepts_press_release() {
        let (tx, rx) = unbounded();
        tx.send(PttSignal::Pressed).unwrap();
        tx.send(PttSignal::Released).unwrap();
        assert_eq!(rx.recv().unwrap(), PttSignal::Pressed);
        assert_eq!(rx.recv().unwrap(), PttSignal::Released);
    }
}
