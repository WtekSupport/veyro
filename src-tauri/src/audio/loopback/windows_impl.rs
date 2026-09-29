use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use std::thread::JoinHandle;

use crossbeam_channel::Sender;
use sysinfo::{ProcessesToUpdate, System};
use tracing::{info, warn};
use wasapi::{
    initialize_mta, AudioClient, DeviceEnumerator, Direction, SampleType, SessionState, StreamMode,
    WaveFormat,
};

use crate::audio::level::{peak_level_percent, update_mic_level};
use crate::audio::monitor::MicMonitor;
use crate::error::AudioError;

use super::{LoopbackAppInfo, LoopbackAppsResponse};

const LOOPBACK_SAMPLE_RATE: u32 = 48_000;
const LOOPBACK_CHANNELS: u16 = 2;

pub struct LoopbackStreamHandle {
    stop_flag: Arc<AtomicBool>,
    _thread: Option<JoinHandle<()>>,
}

impl LoopbackStreamHandle {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

impl Drop for LoopbackStreamHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self._thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn list_loopback_apps() -> LoopbackAppsResponse {
    match list_loopback_apps_inner() {
        Ok(apps) => LoopbackAppsResponse {
            supported: true,
            apps,
        },
        Err(error) => {
            warn!("failed to list loopback apps: {error}");
            LoopbackAppsResponse {
                supported: true,
                apps: Vec::new(),
            }
        }
    }
}

fn list_loopback_apps_inner() -> Result<Vec<LoopbackAppInfo>, String> {
    let _ = initialize_mta();
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut by_pid: HashMap<u32, f32> = HashMap::new();
    let collection = enumerator
        .get_device_collection(&Direction::Render)
        .map_err(|e| e.to_string())?;

    for device in &collection {
        let Ok(dev) = device else {
            continue;
        };
        let Ok(manager) = dev.get_iaudiosessionmanager() else {
            continue;
        };
        let Ok(sessions) = manager.get_audiosessionenumerator() else {
            continue;
        };
        let Ok(count) = sessions.get_count() else {
            continue;
        };
        for index in 0..count {
            let Ok(control) = sessions.get_session(index) else {
                continue;
            };
            let Ok(state) = control.get_state() else {
                continue;
            };
            if state != SessionState::Active && state != SessionState::Inactive {
                continue;
            }
            let Ok(process_id) = control.get_process_id() else {
                continue;
            };
            if process_id == 0 {
                continue;
            }
            let peak = control
                .get_audiometerinformation()
                .and_then(|meter| meter.get_peak_value())
                .unwrap_or(0.0);
            let entry = by_pid.entry(process_id).or_insert(0.0);
            if peak > *entry {
                *entry = peak;
            }
        }
    }

    let mut apps: Vec<LoopbackAppInfo> = by_pid
        .into_iter()
        .map(|(process_id, peak)| {
            let name = process_display_name(&system, process_id);
            LoopbackAppInfo {
                process_id,
                name,
                peak,
            }
        })
        .collect();

    // Prefer apps that are currently producing audio, then by name.
    apps.sort_by(|a, b| {
        b.peak
            .partial_cmp(&a.peak)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
            .then_with(|| a.process_id.cmp(&b.process_id))
    });

    // De-dupe display names that share the same exe by keeping the louder/newer pid.
    let mut seen_names = HashSet::new();
    apps.retain(|app| seen_names.insert(app.name.to_ascii_lowercase()));

    Ok(apps)
}

fn process_display_name(system: &System, process_id: u32) -> String {
    let Some(process) = system.process(sysinfo::Pid::from_u32(process_id)) else {
        return format!("PID {process_id}");
    };
    let name = process.name().to_string_lossy();
    if name.trim().is_empty() {
        format!("PID {process_id}")
    } else {
        name.into_owned()
    }
}

pub fn start_app_loopback_stream(
    process_id: u32,
    label: &str,
    sample_tx: Sender<Vec<f32>>,
    mic_level: Arc<AtomicU32>,
    mic_monitor: Option<MicMonitor>,
) -> Result<(LoopbackStreamHandle, u32, u16, String), AudioError> {
    if process_id == 0 {
        return Err(AudioError::Stream("loopback process id is missing".into()));
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_in_thread = stop_flag.clone();
    let stream_label = format!("{label} (loopback)");
    let label_for_thread = stream_label.clone();

    let thread = std::thread::Builder::new()
        .name("app-loopback".into())
        .spawn(move || {
            if let Err(error) = capture_loop(
                process_id,
                sample_tx,
                mic_level,
                mic_monitor,
                stop_in_thread,
            ) {
                warn!("app loopback capture ended ({label_for_thread}): {error}");
            }
        })
        .map_err(|error| AudioError::Stream(error.to_string()))?;

    info!(
        "app loopback stream started: {} pid={} ({} Hz, {} ch)",
        stream_label, process_id, LOOPBACK_SAMPLE_RATE, LOOPBACK_CHANNELS
    );

    Ok((
        LoopbackStreamHandle {
            stop_flag,
            _thread: Some(thread),
        },
        LOOPBACK_SAMPLE_RATE,
        LOOPBACK_CHANNELS,
        stream_label,
    ))
}

fn capture_loop(
    process_id: u32,
    sample_tx: Sender<Vec<f32>>,
    mic_level: Arc<AtomicU32>,
    mic_monitor: Option<MicMonitor>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), String> {
    let _ = initialize_mta();

    let desired_format = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        LOOPBACK_SAMPLE_RATE as usize,
        LOOPBACK_CHANNELS as usize,
        None,
    );
    let mut audio_client = AudioClient::new_application_loopback_client(process_id, true)
        .map_err(|e| e.to_string())?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };
    audio_client
        .initialize_client(&desired_format, &Direction::Capture, &mode)
        .map_err(|e| e.to_string())?;

    let h_event = audio_client.set_get_eventhandle().map_err(|e| e.to_string())?;
    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| e.to_string())?;
    audio_client.start_stream().map_err(|e| e.to_string())?;

    let mut byte_queue: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let block_align = desired_format.get_blockalign() as usize;

    while !stop_flag.load(Ordering::Relaxed) {
        let new_frames = capture_client
            .get_next_packet_size()
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        if new_frames > 0 {
            let additional = (new_frames as usize * block_align)
                .saturating_sub(byte_queue.capacity().saturating_sub(byte_queue.len()));
            byte_queue.reserve(additional);
            capture_client
                .read_from_device_to_deque(&mut byte_queue)
                .map_err(|e| e.to_string())?;
        }

        // Drain complete frames into f32 samples for the VAD pipeline.
        let frame_bytes = block_align;
        while byte_queue.len() >= frame_bytes * 64 {
            let mut samples = Vec::with_capacity(64 * LOOPBACK_CHANNELS as usize);
            for _ in 0..(64 * LOOPBACK_CHANNELS as usize) {
                if byte_queue.len() < 4 {
                    break;
                }
                let b0 = byte_queue.pop_front().unwrap_or(0);
                let b1 = byte_queue.pop_front().unwrap_or(0);
                let b2 = byte_queue.pop_front().unwrap_or(0);
                let b3 = byte_queue.pop_front().unwrap_or(0);
                samples.push(f32::from_le_bytes([b0, b1, b2, b3]));
            }
            if samples.is_empty() {
                break;
            }
            update_mic_level(&mic_level, &samples);
            if let Some(monitor) = &mic_monitor {
                monitor.push_level(peak_level_percent(&samples));
            }
            let _ = sample_tx.try_send(samples);
        }

        // Short wait so stop_flag is observed quickly; ignore timeout.
        let _ = h_event.wait_for_event(50);
    }

    let _ = audio_client.stop_stream();
    Ok(())
}
