use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter};

static SAMPLING_ENABLED: AtomicBool = AtomicBool::new(false);
static LOOP_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
pub struct AppStats {
    pub cpu_percent: f32,
    pub memory_mb: f64,
    pub process_count: usize,
}

fn collect_process_tree(sys: &System, root: Pid, ncpu: f32) -> AppStats {
    let mut pids: HashSet<Pid> = HashSet::from([root]);
    loop {
        let before = pids.len();
        for (pid, process) in sys.processes() {
            if let Some(parent) = process.parent() {
                if pids.contains(&parent) {
                    pids.insert(*pid);
                }
            }
        }
        if pids.len() == before {
            break;
        }
    }

    let (mut cpu, mut mem) = (0.0f32, 0u64);
    for pid in &pids {
        if let Some(process) = sys.process(*pid) {
            cpu += process.cpu_usage();
            mem += process.memory();
        }
    }

    AppStats {
        cpu_percent: cpu / ncpu,
        memory_mb: mem as f64 / 1024.0 / 1024.0,
        process_count: pids.len(),
    }
}

pub fn set_sampling_enabled(enabled: bool) {
    SAMPLING_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn snapshot_app_stats() -> AppStats {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let root = Pid::from_u32(std::process::id());
    let ncpu = thread::available_parallelism()
        .map(|n| n.get() as f32)
        .unwrap_or(1.0);
    collect_process_tree(&sys, root, ncpu)
}

pub fn start_stats_loop(app: AppHandle) {
    if LOOP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(move || {
        let mut sys = System::new();
        let root = Pid::from_u32(std::process::id());
        let ncpu = thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);

        loop {
            if SAMPLING_ENABLED.load(Ordering::Relaxed) {
                sys.refresh_processes(ProcessesToUpdate::All, true);
                let stats = collect_process_tree(&sys, root, ncpu);
                let _ = app.emit("app-stats", stats);
                thread::sleep(Duration::from_secs(2));
            } else {
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

#[tauri::command]
pub fn set_resource_stats_enabled(enabled: bool) {
    set_sampling_enabled(enabled);
}

#[tauri::command]
pub fn get_app_stats() -> AppStats {
    snapshot_app_stats()
}
