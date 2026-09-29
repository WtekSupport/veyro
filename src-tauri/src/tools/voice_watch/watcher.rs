use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::AppHandle;
use tracing::{info, warn};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::settings::VoiceWatchSettings;
use crate::tools::voice_watch::filter::{filter_candidate, FileAcceptDecision};
use crate::tools::voice_watch::presets::guess_messenger;
use crate::tools::voice_watch::queue::{enqueue_paths, VoiceJobMeta, VoiceJobSource};
use crate::tools::voice_watch::registry::{content_sha256, DedupRegistry};
use crate::tools::voice_watch::stability::{wait_until_stable, StabilityOutcome};

#[derive(Debug)]
enum WatchCmd {
    Stop,
    Update(VoiceWatchSettings),
}

pub struct VoiceWatcherHandle {
    tx: Mutex<Option<mpsc::Sender<WatchCmd>>>,
    running: AtomicBool,
}

impl Default for VoiceWatcherHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceWatcherHandle {
    pub fn new() -> Self {
        Self {
            tx: Mutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        if let Ok(guard) = self.tx.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(WatchCmd::Stop);
            }
        }
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn start_or_update(
        &self,
        app: AppHandle,
        ctx: Arc<AppContext>,
        settings: VoiceWatchSettings,
    ) {
        if !settings.enabled {
            self.stop();
            return;
        }
        let mut guard = self.tx.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = guard.as_ref() {
            if tx.send(WatchCmd::Update(settings.clone())).is_ok() {
                self.running.store(true, Ordering::SeqCst);
                return;
            }
        }
        let (tx, rx) = mpsc::channel::<WatchCmd>();
        *guard = Some(tx);
        self.running.store(true, Ordering::SeqCst);
        let running = Arc::new(AtomicBool::new(true));
        let running_flag = Arc::clone(&running);
        thread::spawn(move || {
            run_watcher_loop(app, ctx, settings, rx, running_flag);
        });
    }
}

fn run_watcher_loop(
    app: AppHandle,
    ctx: Arc<AppContext>,
    mut settings: VoiceWatchSettings,
    rx: mpsc::Receiver<WatchCmd>,
    running: Arc<AtomicBool>,
) {
    let enabled_at = Instant::now();
    let mut debounce: HashMap<PathBuf, Instant> = HashMap::new();
    let mut last_rescan = Instant::now();

    loop {
        let (event_tx, event_rx) = mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(
            move |result| {
                if let Ok(event) = result {
                    let _ = event_tx.send(event);
                }
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(err) => {
                warn!("voice watch init failed: {err}");
                ctx.record_activity(
                    Some(&app),
                    ActivityLevel::Warn,
                    "activity.voiceWatch.watcherError",
                    serde_json::json!({ "error": err.to_string() }),
                );
                running.store(false, Ordering::SeqCst);
                return;
            }
        };

        let mut watched: Vec<(PathBuf, bool)> = Vec::new();
        for folder in &settings.folders {
            let path = PathBuf::from(folder.trim());
            if path.as_os_str().is_empty() {
                continue;
            }
            let exists = path.is_dir();
            let mode = if settings.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if exists {
                if let Err(err) = watcher.watch(&path, mode) {
                    warn!("voice watch failed for {}: {err}", path.display());
                    ctx.record_activity(
                        Some(&app),
                        ActivityLevel::Warn,
                        "activity.voiceWatch.folderUnavailable",
                        serde_json::json!({ "path": path.display().to_string() }),
                    );
                } else {
                    info!("voice watch watching {}", path.display());
                    watched.push((path, true));
                }
            } else {
                watched.push((path, false));
            }
        }

        ctx.record_activity(
            Some(&app),
            ActivityLevel::Info,
            "activity.voiceWatch.status",
            serde_json::json!({
                "enabled": "on",
                "folders": watched.len().to_string(),
            }),
        );

        loop {
            // Control channel
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(WatchCmd::Stop) => {
                    running.store(false, Ordering::SeqCst);
                    return;
                }
                Ok(WatchCmd::Update(new_settings)) => {
                    settings = new_settings;
                    break; // rebuild watches
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            }

            // Drain FS events
            while let Ok(event) = event_rx.try_recv() {
                if !matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
                ) {
                    // Also treat renames as create of destination
                    if !matches!(event.kind, EventKind::Modify(_))
                        && !format!("{:?}", event.kind).contains("Rename")
                        && !format!("{:?}", event.kind).contains("Create")
                    {
                        // accept Rename via string check below
                    }
                }
                let kind_dbg = format!("{:?}", event.kind);
                let interesting = kind_dbg.contains("Create")
                    || kind_dbg.contains("Modify")
                    || kind_dbg.contains("Rename")
                    || matches!(event.kind, EventKind::Any);
                if !interesting {
                    continue;
                }
                for path in event.paths {
                    if settings.recursive {
                        if let Some(depth) = relative_depth(&watched, &path) {
                            if depth > 2 {
                                continue;
                            }
                        }
                    }
                    debounce.insert(path, Instant::now());
                }
            }

            // Process debounced paths
            let ready: Vec<PathBuf> = debounce
                .iter()
                .filter(|(_, t)| t.elapsed() >= Duration::from_millis(400))
                .map(|(p, _)| p.clone())
                .collect();
            for path in ready {
                debounce.remove(&path);
                // Only files created after enable
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                            // If file is older than watcher uptime + small slack, skip (pre-existing)
                            if enabled_at.elapsed() < Duration::from_secs(2)
                                && elapsed > enabled_at.elapsed() + Duration::from_secs(5)
                            {
                                continue;
                            }
                        }
                    }
                }
                let app2 = app.clone();
                let ctx2 = ctx.clone();
                let settings2 = settings.clone();
                let path_work = path;
                thread::spawn(move || {
                    handle_candidate(&app2, ctx2, &settings2, &path_work);
                });
            }

            // Rare rescan for unavailable folders
            if last_rescan.elapsed() > Duration::from_secs(30) {
                last_rescan = Instant::now();
                let mut need_rebuild = false;
                for (path, was_ok) in &watched {
                    let now_ok = path.is_dir();
                    if *was_ok != now_ok {
                        need_rebuild = true;
                        break;
                    }
                }
                if need_rebuild {
                    break;
                }
            }
        }
    }
}

fn relative_depth(watched: &[(PathBuf, bool)], path: &Path) -> Option<usize> {
    for (root, _) in watched {
        if let Ok(rel) = path.strip_prefix(root) {
            return Some(rel.components().count().saturating_sub(1));
        }
    }
    None
}

fn handle_candidate(
    app: &AppHandle,
    ctx: Arc<AppContext>,
    settings: &VoiceWatchSettings,
    path: &Path,
) {
    if !path.is_file() {
        return;
    }

    ctx.record_activity(
        Some(app),
        ActivityLevel::Info,
        "activity.voiceWatch.detected",
        serde_json::json!({
            "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        }),
    );

    let outcome = wait_until_stable(path, settings.stable_ms);
    let (size, mtime) = match outcome {
        StabilityOutcome::Stable { size, mtime } => (size, mtime),
        StabilityOutcome::Disappeared => {
            return;
        }
        StabilityOutcome::TimedOut => {
            ctx.record_activity(
                Some(app),
                ActivityLevel::Warn,
                "activity.voiceWatch.readTimeout",
                serde_json::json!({
                    "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                }),
            );
            return;
        }
    };

    match filter_candidate(path, settings, size) {
        FileAcceptDecision::Accept => {}
        other => {
            info!("voice watch skip {:?}: {:?}", other, path);
            return;
        }
    }

    let path_str = path.to_string_lossy().to_string();
    let sha = match content_sha256(path) {
        Ok(s) => s,
        Err(err) => {
            warn!("voice watch hash failed: {err}");
            return;
        }
    };

    let reg = DedupRegistry::load();
    if reg.is_duplicate(&path_str, size, mtime, &sha) {
        info!("voice watch dedup skip {}", path.display());
        return;
    }

    // Light audio probe: try opening via decode path existence; mark not-audio on hard fail later.
    // Duration gate: estimate from file — skip if we can probe; otherwise enqueue and let pipeline handle.
    if let Some(secs) = probe_duration_secs(path) {
        if secs > settings.max_duration_secs() as f64 {
            use crate::tools::voice_watch::history::{
                new_history_id, touch_now_ms, upsert_history_entry, HistoryEntry, HistoryStatus,
            };
            let entry = HistoryEntry {
                id: new_history_id(),
                path: path_str,
                file_name: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
                source: "auto-watch".to_string(),
                messenger: guess_messenger(path),
                appeared_at_ms: touch_now_ms(),
                updated_at_ms: touch_now_ms(),
                duration_secs: Some(secs),
                status: HistoryStatus::TooLong,
                text: String::new(),
                error_key: Some("tools.voiceWatch.tooLong".to_string()),
                content_sha256: Some(sha),
            };
            let _ = upsert_history_entry(entry);
            ctx.record_activity(
                Some(app),
                ActivityLevel::Info,
                "activity.voiceWatch.tooLong",
                serde_json::json!({}),
            );
            return;
        }
    }

    let meta = VoiceJobMeta {
        source: VoiceJobSource::AutoWatch,
        messenger: guess_messenger(path),
        appeared_at_ms: Some(
            crate::tools::voice_watch::history::touch_now_ms(),
        ),
        content_sha256: Some(sha),
        stt_language_override: None,
    };

    if let Err(err) = enqueue_paths(app, ctx, vec![path_str], meta) {
        warn!("voice watch enqueue failed: {err}");
    }
}

fn probe_duration_secs(path: &Path) -> Option<f64> {
    use std::fs::File;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let track = probed.format.default_track()?;
    let params = &track.codec_params;
    let rate = params.sample_rate? as f64;
    let frames = params.n_frames?;
    Some(frames as f64 / rate)
}
