use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use tracing::{info, warn};

use crate::audio::capture::{CaptureMode, SegmentCallback};
use crate::audio::level::decay_mic_level;
use crate::audio::monitor::MicMonitor;
use crate::audio::segment::AudioSegment;
use crate::audio::stream::start_input_stream;
use crate::error::AudioError;
use crate::vad::{VadConfig, VadDetector, VadEvent};

const AUDIO_CHANNEL_CAPACITY: usize = 64;
/// Time to wait for VAD to flush a PTT segment after key release.
pub const PTT_FLUSH_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

type PreviewSenderSlot = Arc<Mutex<Option<Sender<AudioSegment>>>>;

fn dispatch_preview_snapshot(
    tx_slot: &PreviewSenderSlot,
    detector: &VadDetector,
    last_preview_at: &mut std::time::Instant,
    preview_interval: std::time::Duration,
) {
    if last_preview_at.elapsed() < preview_interval {
        return;
    }
    let Some(segment) = detector.preview_snapshot() else {
        return;
    };
    let sender = tx_slot.lock().ok().and_then(|guard| guard.clone());
    let Some(sender) = sender else {
        return;
    };
    match sender.try_send(segment) {
        Ok(()) => {
            *last_preview_at = std::time::Instant::now();
        }
        Err(crossbeam_channel::TrySendError::Full(_)) => {
            *last_preview_at = std::time::Instant::now();
        }
        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
            warn!("live preview channel disconnected");
        }
    }
}

enum AudioCommand {
    StartCapture {
        device_id: Option<String>,
        audio_tx: Sender<Vec<f32>>,
        mic_level: Arc<AtomicU32>,
        mic_monitor: MicMonitor,
    },
    StartLevelMonitor {
        device_id: Option<String>,
        mic_level: Arc<AtomicU32>,
        mic_monitor: MicMonitor,
    },
    StopCapture,
    WarmDevice {
        device_id: Option<String>,
        mic_level: Arc<AtomicU32>,
        mic_monitor: MicMonitor,
    },
    Shutdown,
}

/// Send + Sync handle; cpal stream lives on the dedicated audio thread.
pub struct AudioPipeline {
    mode: CaptureMode,
    vad_config: VadConfig,
    device_id: Option<String>,
    running: Arc<AtomicBool>,
    ptt_active: Arc<AtomicBool>,
    ptt_gate: Arc<AtomicBool>,
    /// When true, VAD may end segments on silence while PTT gate is held (deferred AI postprocess).
    ptt_vad_segments_on_silence: Arc<AtomicBool>,
    speech_active: Arc<AtomicBool>,
    mic_level: Arc<AtomicU32>,
    input_stream_active: Arc<AtomicBool>,
    active_input_name: Arc<Mutex<Option<String>>>,
    cmd_tx: Sender<AudioCommand>,
    audio_thread: Option<std::thread::JoinHandle<()>>,
    vad_thread: Option<std::thread::JoinHandle<()>>,
    vad_session_active: Option<Arc<AtomicBool>>,
    segment_rx: Option<Receiver<AudioSegment>>,
    ptt_flush_rx: Option<Receiver<AudioSegment>>,
    ptt_flush_req_tx: Option<Sender<()>>,
    on_speech_started: Option<Arc<dyn Fn() + Send + Sync>>,
    on_speech_ended: Option<SegmentCallback>,
    preview_sender: PreviewSenderSlot,
    mic_monitor: MicMonitor,
}

impl AudioPipeline {
    pub fn new(vad_config: VadConfig) -> Self {
        let (cmd_tx, cmd_rx) = bounded(8);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();
        let input_stream_active = Arc::new(AtomicBool::new(false));
        let stream_active_flag = input_stream_active.clone();
        let active_input_name = Arc::new(Mutex::new(None));
        let active_input_name_thread = active_input_name.clone();

        let audio_thread = std::thread::Builder::new()
            .name("audio-stream".into())
            .spawn(move || {
                audio_thread_main(
                    cmd_rx,
                    shutdown_flag,
                    stream_active_flag,
                    active_input_name_thread,
                )
            })
            .ok();

        Self {
            mode: CaptureMode::Continuous,
            vad_config,
            device_id: None,
            running: Arc::new(AtomicBool::new(false)),
            ptt_active: Arc::new(AtomicBool::new(false)),
            ptt_gate: Arc::new(AtomicBool::new(false)),
            ptt_vad_segments_on_silence: Arc::new(AtomicBool::new(false)),
            speech_active: Arc::new(AtomicBool::new(false)),
            mic_level: Arc::new(AtomicU32::new(0)),
            input_stream_active,
            active_input_name,
            cmd_tx,
            audio_thread,
            vad_thread: None,
            vad_session_active: None,
            segment_rx: None,
            ptt_flush_rx: None,
            ptt_flush_req_tx: None,
            on_speech_started: None,
            on_speech_ended: None,
            preview_sender: Arc::new(Mutex::new(None)),
            mic_monitor: MicMonitor::new(),
        }
    }

    pub fn set_mic_monitor(&mut self, monitor: MicMonitor) {
        self.mic_monitor = monitor;
    }

    pub fn mic_monitor(&self) -> MicMonitor {
        self.mic_monitor.clone()
    }

    pub fn set_vad_config(&mut self, config: VadConfig) {
        self.vad_config = config;
    }

    pub fn set_callbacks(
        &mut self,
        on_speech_started: Arc<dyn Fn() + Send + Sync>,
        on_speech_ended: SegmentCallback,
    ) {
        self.on_speech_started = Some(on_speech_started);
        self.on_speech_ended = Some(on_speech_ended);
    }

    pub fn set_ptt_vad_segments_on_silence(&self, enabled: bool) {
        self.ptt_vad_segments_on_silence
            .store(enabled, Ordering::SeqCst);
    }

    pub fn set_preview_sender(&mut self, sender: Sender<AudioSegment>) {
        if let Ok(mut slot) = self.preview_sender.lock() {
            *slot = Some(sender);
        } else {
            warn!("failed to store live preview sender: lock poisoned");
        }
    }

    pub fn start(
        &mut self,
        device_id: Option<String>,
        mode: CaptureMode,
    ) -> Result<(), AudioError> {
        if self.running.load(Ordering::SeqCst) {
            if let Some(thread) = self.stop()? {
                let _ = thread.join();
            }
        }

        self.device_id = device_id;
        self.mode = mode;
        self.running.store(true, Ordering::SeqCst);

        if mode == CaptureMode::PushToTalk {
            self.ptt_active.store(false, Ordering::SeqCst);
            self.ptt_gate.store(false, Ordering::SeqCst);
            self.start_capture_pipeline()?;
            info!("audio pipeline armed for push-to-talk (standby capture active)");
            return Ok(());
        }

        self.ptt_gate.store(true, Ordering::SeqCst);
        self.start_capture_pipeline()?;
        info!("audio pipeline started in continuous mode");
        Ok(())
    }

    pub fn prewarm(&mut self, device_id: Option<String>) -> Result<(), AudioError> {
        self.device_id = device_id;
        let (sample_rate, channels) =
            crate::audio::warmup::query_device_format(self.device_id.as_deref())?;

        crate::audio::warmup::warm_vad_detector(self.vad_config.clone(), sample_rate, channels);

        // Never stop an active capture stream — WarmDevice tears down cpal input.
        // A live VAD worker also means the pipeline is armed even if `is_capturing` flickers.
        let stream_active = self.input_stream_active.load(Ordering::Relaxed);
        let pipeline_armed = self.running.load(Ordering::Relaxed)
            && (stream_active || self.is_capturing());
        if pipeline_armed {
            info!(
                "microphone prewarmed in memory (capture stream kept alive): {} ({sample_rate} Hz, {channels} ch)",
                self.device_id.as_deref().unwrap_or("default")
            );
            return Ok(());
        }

        if !stream_active {
            self.cmd_tx
                .send(AudioCommand::WarmDevice {
                    device_id: self.device_id.clone(),
                    mic_level: Arc::clone(&self.mic_level),
                    mic_monitor: self.mic_monitor.clone(),
                })
                .map_err(|error| AudioError::Stream(error.to_string()))?;
        }

        info!(
            "microphone prewarmed: {} ({sample_rate} Hz, {channels} ch)",
            self.device_id.as_deref().unwrap_or("default")
        );
        Ok(())
    }

    pub fn stop(&mut self) -> Result<Option<std::thread::JoinHandle<()>>, AudioError> {
        self.running.store(false, Ordering::SeqCst);
        self.ptt_active.store(false, Ordering::SeqCst);
        self.ptt_gate.store(false, Ordering::SeqCst);
        self.ptt_vad_segments_on_silence
            .store(false, Ordering::SeqCst);
        self.speech_active.store(false, Ordering::Relaxed);
        self.mic_level.store(0, Ordering::Relaxed);
        let _ = self.cmd_tx.send(AudioCommand::StopCapture);
        let vad_join = self.take_vad_worker();

        self.segment_rx = None;
        self.ptt_flush_rx = None;
        self.ptt_flush_req_tx = None;
        info!("audio pipeline stopped");
        Ok(vad_join)
    }

    pub fn set_ptt_active(
        &mut self,
        active: bool,
    ) -> Result<Option<std::thread::JoinHandle<()>>, AudioError> {
        if self.mode != CaptureMode::PushToTalk || !self.running.load(Ordering::SeqCst) {
            return Ok(None);
        }

        if !active {
            return Ok(None);
        }

        let was_active = self.ptt_active.swap(true, Ordering::SeqCst);
        if !was_active {
            self.drain_pending_segments();
        }

        self.ptt_gate.store(true, Ordering::SeqCst);
        // Must be set before VAD worker (re)start so silence splits utterances while PTT is held.
        self.ptt_vad_segments_on_silence
            .store(true, Ordering::SeqCst);

        let stream_active = self.input_stream_active.load(Ordering::Relaxed);
        let vad_restart = self.vad_worker_needs_restart();
        if vad_restart || !stream_active {
            info!(
                vad_restart,
                stream_active,
                "restarting capture pipeline for push-to-talk press"
            );
            self.start_capture_pipeline()?;
        }

        Ok(None)
    }

    pub fn is_input_stream_active(&self) -> bool {
        self.input_stream_active.load(Ordering::Relaxed)
    }

    /// Starts PTT release: clears gate, requests VAD flush, returns the flush receiver.
    /// Caller should wait on the receiver outside `audio` lock, then call `restore_ptt_flush_rx`.
    pub fn begin_ptt_release(&mut self) -> Result<Option<Receiver<AudioSegment>>, AudioError> {
        if self.mode != CaptureMode::PushToTalk || !self.running.load(Ordering::SeqCst) {
            return Ok(None);
        }

        let was_active = self.ptt_active.swap(false, Ordering::SeqCst);
        self.ptt_gate.store(false, Ordering::SeqCst);
        self.ptt_vad_segments_on_silence
            .store(false, Ordering::SeqCst);

        if !was_active {
            return Ok(self.ptt_flush_rx.take());
        }

        if let Some(tx) = &self.ptt_flush_req_tx {
            let _ = tx.try_send(());
        }

        Ok(self.ptt_flush_rx.take())
    }

    pub fn restore_ptt_flush_rx(&mut self, rx: Receiver<AudioSegment>) {
        self.ptt_flush_rx = Some(rx);
    }

    /// Waits for the VAD worker to deliver a flushed PTT segment.
    pub fn wait_for_ptt_flush(
        rx: Receiver<AudioSegment>,
    ) -> (Option<AudioSegment>, Receiver<AudioSegment>) {
        let deadline = std::time::Instant::now() + PTT_FLUSH_WAIT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(segment) => return (Some(segment), rx),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        }
        (rx.try_recv().ok(), rx)
    }

    pub fn poll_ptt_segment(&self) -> Option<AudioSegment> {
        self.poll_segment().filter(|segment| !segment.is_empty())
    }

    /// Drop segments queued for continuous-mode callbacks. PTT must not reuse stale audio.
    pub fn drain_pending_segments(&mut self) {
        let Some(rx) = &self.segment_rx else {
            return;
        };
        while rx.try_recv().is_ok() {}
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn is_capturing(&self) -> bool {
        self.vad_thread
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    pub fn is_speech_active(&self) -> bool {
        self.speech_active.load(Ordering::Relaxed)
    }

    pub fn is_ptt_gate_open(&self) -> bool {
        self.ptt_gate.load(Ordering::Relaxed)
    }

    pub fn poll_segment(&self) -> Option<AudioSegment> {
        self.segment_rx.as_ref()?.try_recv().ok()
    }

    pub fn mic_level(&self) -> u32 {
        crate::audio::level::mic_level_percent(&self.mic_level)
    }

    pub fn mic_level_atomic(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.mic_level)
    }

    pub fn active_input_device(&self) -> Option<String> {
        self.active_input_name
            .lock()
            .ok()
            .and_then(|name| name.clone())
    }

    /// Keep an input stream open for the settings mic meter when capture is idle.
    pub fn ensure_level_monitor(
        &mut self,
        device_id: Option<String>,
    ) -> Result<(), AudioError> {
        // Never steal or race the capture stream used by VAD / push-to-talk.
        if self.running.load(Ordering::Relaxed)
            || self.is_capturing()
            || self.input_stream_active.load(Ordering::Relaxed)
        {
            return Ok(());
        }

        self.device_id = device_id.clone();
        self.cmd_tx
            .send(AudioCommand::StartLevelMonitor {
                device_id,
                mic_level: Arc::clone(&self.mic_level),
                mic_monitor: self.mic_monitor.clone(),
            })
            .map_err(|error| AudioError::Stream(error.to_string()))
    }

    fn vad_worker_needs_restart(&mut self) -> bool {
        match self.vad_thread.as_ref() {
            None => true,
            Some(handle) if handle.is_finished() => {
                self.vad_thread = None;
                true
            }
            Some(_) => false,
        }
    }

    fn take_vad_worker(&mut self) -> Option<std::thread::JoinHandle<()>> {
        if let Some(flag) = self.vad_session_active.take() {
            flag.store(false, Ordering::SeqCst);
        }
        self.vad_thread.take()
    }

    fn start_capture_pipeline(&mut self) -> Result<(), AudioError> {
        if let Some(thread) = self.take_vad_worker() {
            std::thread::spawn(move || {
                let _ = thread.join();
            });
        }

        let (audio_tx, audio_rx) = bounded(AUDIO_CHANNEL_CAPACITY);
        let (segment_tx, segment_rx) = bounded(4);
        let (ptt_flush_tx, ptt_flush_rx) = bounded(1);
        let (ptt_flush_req_tx, ptt_flush_req_rx) = bounded(1);
        self.ptt_flush_rx = Some(ptt_flush_rx);
        self.ptt_flush_req_tx = Some(ptt_flush_req_tx);

        let (sample_rate, channels) =
            crate::audio::warmup::query_device_format(self.device_id.as_deref())?;
        crate::audio::warmup::warm_vad_detector(self.vad_config.clone(), sample_rate, channels);
        let ptt_gate = Arc::clone(&self.ptt_gate);
        let ptt_vad_segments_on_silence = Arc::clone(&self.ptt_vad_segments_on_silence);
        let ptt_mode = self.mode == CaptureMode::PushToTalk;

        self.cmd_tx
            .send(AudioCommand::StartCapture {
                device_id: self.device_id.clone(),
                audio_tx,
                mic_level: Arc::clone(&self.mic_level),
                mic_monitor: self.mic_monitor.clone(),
            })
            .map_err(|error| AudioError::Stream(error.to_string()))?;

        let session_active = Arc::new(AtomicBool::new(true));
        let session_flag = session_active.clone();
        self.vad_session_active = Some(session_active);
        let speech_active = Arc::clone(&self.speech_active);
        let on_speech_started = self.on_speech_started.clone();
        let on_speech_ended = self.on_speech_ended.clone();
        let preview_sender = Arc::clone(&self.preview_sender);
        if preview_sender
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .is_none()
        {
            warn!("VAD worker started without live preview sender");
        }
        let vad_config = self.vad_config.clone();
        let worker = std::thread::Builder::new()
            .name("vad-worker".into())
            .spawn(move || {
                let mut detector = VadDetector::new(vad_config, sample_rate, channels);
                let mut gate_was_active = ptt_gate.load(Ordering::SeqCst);
                let preview_interval = std::time::Duration::from_millis(
                    crate::audio::preview::PREVIEW_POLL_INTERVAL_MS,
                );
                let mut last_preview_at = std::time::Instant::now()
                    .checked_sub(preview_interval)
                    .unwrap_or_else(std::time::Instant::now);
                if ptt_mode && gate_was_active {
                    detector.set_ptt_recording(true);
                }

                while session_flag.load(Ordering::SeqCst) {
                    let gate_active = ptt_gate.load(Ordering::SeqCst);
                    let segment_on_silence = ptt_vad_segments_on_silence.load(Ordering::Relaxed);
                    let end_on_silence = if ptt_mode {
                        gate_active && segment_on_silence
                    } else {
                        true
                    };
                    detector.set_end_on_silence(end_on_silence);

                    let flush_requested = ptt_flush_req_rx.try_recv().is_ok();
                    let gate_released =
                        ptt_mode && gate_active != gate_was_active && !gate_active;

                    if flush_requested || gate_released {
                        if ptt_mode {
                            detector.set_ptt_recording(true);
                            while let Ok(samples) = audio_rx.try_recv() {
                                if let Err(error) = detector.push_samples(&samples) {
                                    warn!("vad pre-flush drain failed: {error}");
                                }
                            }
                        }
                        let flush_result = if ptt_mode {
                            detector.flush_ptt()
                        } else {
                            detector.flush()
                        };
                        if let Ok(Some(VadEvent::SpeechEnded(segment))) = flush_result {
                            let _ = ptt_flush_tx.try_send(segment);
                        }
                        speech_active.store(detector.is_speaking(), Ordering::Relaxed);
                        if ptt_mode {
                            detector.set_ptt_recording(false);
                        }
                        gate_was_active = gate_active;
                        continue;
                    }

                    if ptt_mode {
                        if gate_active && !gate_was_active {
                            detector.reset();
                        }
                        detector.set_ptt_recording(gate_active);
                        if gate_active != gate_was_active {
                            gate_was_active = gate_active;
                        }
                    }

                    match audio_rx.try_recv() {
                        Ok(samples) => {
                            if ptt_mode && !ptt_gate.load(Ordering::SeqCst) {
                                continue;
                            }
                            match detector.push_samples(&samples) {
                                Ok(events) => {
                                    for event in events {
                                        match event {
                                            VadEvent::SpeechStarted => {
                                                if let Some(callback) = &on_speech_started {
                                                    callback();
                                                }
                                            }
                                            VadEvent::SpeechEnded(segment) => {
                                                if ptt_mode {
                                                    if gate_active && segment_on_silence {
                                                        // Mid-PTT chunk: one STT+inject per silence (like continuous mode).
                                                        if let Some(callback) = &on_speech_ended {
                                                            callback(segment);
                                                        }
                                                    }
                                                } else {
                                                    let _ = segment_tx.try_send(segment.clone());
                                                    if let Some(callback) = &on_speech_ended {
                                                        callback(segment);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    speech_active
                                        .store(detector.is_speaking(), Ordering::Relaxed);
                                }
                                Err(error) => warn!("vad processing failed: {error}"),
                            }
                        }
                        Err(TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Err(TryRecvError::Disconnected) => break,
                    }

                    if !ptt_mode || ptt_gate.load(Ordering::SeqCst) {
                        dispatch_preview_snapshot(
                            &preview_sender,
                            &detector,
                            &mut last_preview_at,
                            preview_interval,
                        );
                    }
                }

                if let Ok(Some(VadEvent::SpeechEnded(segment))) = detector.flush() {
                    let _ = segment_tx.try_send(segment.clone());
                    if let Some(callback) = &on_speech_ended {
                        callback(segment);
                    }
                }
                speech_active.store(false, Ordering::Relaxed);

            })
            .map_err(|error| AudioError::Stream(error.to_string()))?;

        self.vad_thread = Some(worker);
        self.segment_rx = Some(segment_rx);
        Ok(())
    }
}

impl Drop for AudioPipeline {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(AudioCommand::Shutdown);
        if let Ok(Some(thread)) = self.stop() {
            let _ = thread.join();
        }
        if let Some(thread) = self.audio_thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    None,
    Capture,
    Monitor,
}

fn set_active_input_name(active_input_name: &Mutex<Option<String>>, name: Option<String>) {
    if let Ok(mut guard) = active_input_name.lock() {
        *guard = name;
    }
}

/// WASAPI often returns 0x80070057 if capture is reopened immediately after stop.
fn start_input_stream_resilient(
    device_id: Option<&str>,
    sample_tx: Sender<Vec<f32>>,
    mic_level: Arc<AtomicU32>,
    mic_monitor: MicMonitor,
    after_restart: bool,
) -> Result<(crate::audio::stream::AudioStreamHandle, u32, u16, String), AudioError> {
    if after_restart {
        #[cfg(windows)]
        std::thread::sleep(std::time::Duration::from_millis(120));
    }

    #[cfg(windows)]
    {
        match start_input_stream(
            device_id,
            sample_tx.clone(),
            mic_level.clone(),
            Some(mic_monitor.clone()),
        ) {
            Ok(result) => Ok(result),
            Err(first) => {
                warn!("input stream start failed ({first}), retrying once after brief delay");
                std::thread::sleep(std::time::Duration::from_millis(200));
                start_input_stream(device_id, sample_tx, mic_level, Some(mic_monitor))
            }
        }
    }
    #[cfg(not(windows))]
    {
        start_input_stream(
            device_id,
            sample_tx,
            mic_level,
            Some(mic_monitor),
        )
    }
}

fn audio_thread_main(
    cmd_rx: Receiver<AudioCommand>,
    shutdown: Arc<AtomicBool>,
    input_stream_active: Arc<AtomicBool>,
    active_input_name: Arc<Mutex<Option<String>>>,
) {
    let mut stream_handle: Option<crate::audio::stream::AudioStreamHandle> = None;
    let mut active_mic_level: Option<Arc<AtomicU32>> = None;
    let mut stream_kind = StreamKind::None;

    while !shutdown.load(Ordering::SeqCst) {
        match cmd_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(AudioCommand::StartCapture {
                device_id,
                audio_tx,
                mic_level,
                mic_monitor,
            }) => {
                let after_restart = stream_handle.is_some();
                stop_input_stream(
                    stream_handle.take(),
                    &input_stream_active,
                    &mut active_mic_level,
                    &active_input_name,
                );
                stream_kind = StreamKind::None;
                match start_input_stream_resilient(
                    device_id.as_deref(),
                    audio_tx,
                    mic_level.clone(),
                    mic_monitor,
                    after_restart,
                ) {
                    Ok((handle, rate, channels, device_name)) => {
                        info!(
                            "audio input stream started: {device_name} ({rate} Hz, {channels} ch)"
                        );
                        stream_handle = Some(handle);
                        active_mic_level = Some(mic_level);
                        stream_kind = StreamKind::Capture;
                        set_active_input_name(&active_input_name, Some(device_name));
                        input_stream_active.store(true, Ordering::Relaxed);
                    }
                    Err(error) => {
                        input_stream_active.store(false, Ordering::Relaxed);
                        warn!(
                            "failed to start input stream for {}: {error}",
                            device_id.as_deref().unwrap_or("default")
                        );
                    }
                }
            }
            Ok(AudioCommand::StartLevelMonitor {
                device_id,
                mic_level,
                mic_monitor,
            }) => {
                if stream_kind == StreamKind::Capture {
                    info!("mic level monitor skipped — capture stream already active");
                    continue;
                }
                stop_input_stream(
                    stream_handle.take(),
                    &input_stream_active,
                    &mut active_mic_level,
                    &active_input_name,
                );
                stream_kind = StreamKind::None;
                let (discard_tx, _discard_rx) = bounded(4);
                match start_input_stream(
                    device_id.as_deref(),
                    discard_tx,
                    mic_level.clone(),
                    Some(mic_monitor),
                ) {
                    Ok((handle, rate, channels, device_name)) => {
                        info!(
                            "mic level monitor started: {device_name} ({rate} Hz, {channels} ch)"
                        );
                        stream_handle = Some(handle);
                        active_mic_level = Some(mic_level);
                        stream_kind = StreamKind::Monitor;
                        set_active_input_name(&active_input_name, Some(device_name));
                        input_stream_active.store(true, Ordering::Relaxed);
                    }
                    Err(error) => warn!(
                        "failed to start mic level monitor for {}: {error}",
                        device_id.as_deref().unwrap_or("default")
                    ),
                }
            }
            Ok(AudioCommand::StopCapture) => {
                stop_input_stream(
                    stream_handle.take(),
                    &input_stream_active,
                    &mut active_mic_level,
                    &active_input_name,
                );
                stream_kind = StreamKind::None;
            }
            Ok(AudioCommand::WarmDevice {
                device_id,
                mic_level,
                mic_monitor,
            }) => {
                stop_input_stream(
                    stream_handle.take(),
                    &input_stream_active,
                    &mut active_mic_level,
                    &active_input_name,
                );
                stream_kind = StreamKind::None;
                let (discard_tx, discard_rx) = bounded(4);
                match start_input_stream(
                    device_id.as_deref(),
                    discard_tx,
                    mic_level,
                    Some(mic_monitor),
                ) {
                    Ok((handle, _, _, _)) => {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        handle.stop();
                        while discard_rx.try_recv().is_ok() {}
                    }
                    Err(error) => warn!("device warm-up failed: {error}"),
                }
            }
            Ok(AudioCommand::Shutdown) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if let Some(level) = &active_mic_level {
                    decay_mic_level(level);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    stop_input_stream(
        stream_handle.take(),
        &input_stream_active,
        &mut active_mic_level,
        &active_input_name,
    );
}

fn stop_input_stream(
    handle: Option<crate::audio::stream::AudioStreamHandle>,
    input_stream_active: &AtomicBool,
    active_mic_level: &mut Option<Arc<AtomicU32>>,
    active_input_name: &Mutex<Option<String>>,
) {
    if let Some(handle) = handle {
        handle.stop();
    }
    input_stream_active.store(false, Ordering::Relaxed);
    *active_mic_level = None;
    set_active_input_name(active_input_name, None);
}
