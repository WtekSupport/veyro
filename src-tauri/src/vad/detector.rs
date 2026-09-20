use crate::audio::resampler::{MonoResampler, TARGET_SAMPLE_RATE};
use crate::audio::ring_buffer::RingBuffer;
use crate::audio::segment::AudioSegment;
use super::analyzer::VoiceAnalyzer;
use super::config::VadConfig;

#[cfg(test)]
const WEBRTC_FRAME_MS: u32 = 20;
#[cfg(test)]
const WEBRTC_FRAME_SAMPLES: usize = (TARGET_SAMPLE_RATE as usize * WEBRTC_FRAME_MS as usize) / 1000;

#[derive(Debug, Clone)]
pub enum VadEvent {
    SpeechStarted,
    SpeechEnded(AudioSegment),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VadState {
    Idle,
    Speaking,
}

pub struct VadDetector {
    config: VadConfig,
    analyzer: VoiceAnalyzer,
    frame_samples: usize,
    state: VadState,
    pre_buffer: RingBuffer,
    segment: Vec<f32>,
    pending: Vec<f32>,
    silence_samples: usize,
    source_channels: u16,
    resampler: MonoResampler,
    end_on_silence: bool,
    ptt_recording: bool,
    ptt_capture: Vec<f32>,
    ptt_subsegments_sent: bool,
}

impl VadDetector {
    pub fn new(config: VadConfig, source_rate: u32, source_channels: u16) -> Self {
        let threshold = config.effective_threshold_percent();
        let analyzer = VoiceAnalyzer::new(config.engine, threshold);
        let frame_samples = frame_samples_from_ms(analyzer.frame_ms());
        let pre_capacity = config.pre_speech_samples(TARGET_SAMPLE_RATE).max(frame_samples);
        let resampler = MonoResampler::new(source_rate, TARGET_SAMPLE_RATE)
            .expect("failed to create mono resampler");
        Self {
            analyzer,
            frame_samples,
            config,
            state: VadState::Idle,
            pre_buffer: RingBuffer::new(pre_capacity),
            segment: Vec::new(),
            pending: Vec::new(),
            silence_samples: 0,
            source_channels,
            resampler,
            end_on_silence: true,
            ptt_recording: false,
            ptt_capture: Vec::new(),
            ptt_subsegments_sent: false,
        }
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    pub fn set_end_on_silence(&mut self, enabled: bool) {
        self.end_on_silence = enabled;
    }

    pub fn set_ptt_recording(&mut self, enabled: bool) {
        self.ptt_recording = enabled;
    }

    pub fn reset(&mut self) {
        self.state = VadState::Idle;
        self.segment.clear();
        self.pending.clear();
        self.pre_buffer.clear();
        self.ptt_capture.clear();
        self.ptt_subsegments_sent = false;
        self.silence_samples = 0;
        self.resampler.reset();
    }

    fn mark_ptt_subsegment_delivered(&mut self) {
        if self.ptt_recording {
            self.ptt_subsegments_sent = true;
        }
    }

    pub fn push_samples(&mut self, samples: &[f32]) -> Result<Vec<VadEvent>, crate::error::AudioError> {
        let normalized = self
            .resampler
            .push(samples, self.source_channels)?;
        if self.ptt_recording {
            self.ptt_capture.extend_from_slice(&normalized);
        }
        self.pending.extend_from_slice(&normalized);
        let mut events = Vec::new();

        while self.pending.len() >= self.frame_samples {
            let frame: Vec<f32> = self.pending.drain(..self.frame_samples).collect();
            if let Some(event) = self.process_frame(&frame)? {
                events.push(event);
            }
        }

        Ok(events)
    }

    pub fn flush(&mut self) -> Result<Option<VadEvent>, crate::error::AudioError> {
        self.drain_pending_frames()?;

        if self.state == VadState::Speaking && !self.segment.is_empty() {
            let segment = self.take_segment()?;
            self.state = VadState::Idle;
            return Ok(Some(VadEvent::SpeechEnded(segment)));
        }
        Ok(None)
    }

    /// Flush buffered audio on PTT key release even if VAD never entered Speaking.
    /// Clone current in-progress audio for live transcription preview.
    pub fn preview_snapshot(&self) -> Option<AudioSegment> {
        let min_samples = crate::audio::preview::preview_min_samples(TARGET_SAMPLE_RATE);

        let segment = if self.state == VadState::Speaking && self.segment.len() >= min_samples {
            AudioSegment::new(self.segment.clone(), TARGET_SAMPLE_RATE, 1)
        } else if self.ptt_recording && self.ptt_capture.len() >= min_samples {
            AudioSegment::new(self.ptt_capture.clone(), TARGET_SAMPLE_RATE, 1)
        } else {
            return None;
        };

        crate::audio::preview::trim_for_live_dictation(&segment)
    }

    pub fn flush_ptt(&mut self) -> Result<Option<VadEvent>, crate::error::AudioError> {
        self.drain_pending_frames()?;

        if self.ptt_subsegments_sent {
            self.ptt_subsegments_sent = false;
            let min_speech = self.config.minimum_speech_samples(TARGET_SAMPLE_RATE);
            if self.state == VadState::Speaking && self.segment.len() >= min_speech {
                let segment = self.take_segment()?;
                self.ptt_recording = false;
                self.ptt_capture.clear();
                return Ok(Some(VadEvent::SpeechEnded(segment)));
            }
            self.ptt_recording = false;
            self.ptt_capture.clear();
            self.reset();
            return Ok(None);
        }

        let ptt_min_samples = (TARGET_SAMPLE_RATE as usize * 100) / 1000;
        if self.ptt_capture.len() >= ptt_min_samples {
            let samples = std::mem::take(&mut self.ptt_capture);
            self.ptt_recording = false;
            self.reset();
            return Ok(Some(VadEvent::SpeechEnded(AudioSegment::new(
                samples,
                TARGET_SAMPLE_RATE,
                1,
            ))));
        }

        if !self.segment.is_empty() {
            let segment = self.take_segment()?;
            self.ptt_recording = false;
            self.state = VadState::Idle;
            return Ok(Some(VadEvent::SpeechEnded(segment)));
        }

        let pre = self.pre_buffer.read_last(self.pre_buffer.len());
        let required = self
            .config
            .minimum_speech_samples(TARGET_SAMPLE_RATE)
            .min(ptt_min_samples.max(self.frame_samples));
        if pre.len() >= required {
            self.ptt_recording = false;
            self.reset();
            return Ok(Some(VadEvent::SpeechEnded(AudioSegment::new(
                pre,
                TARGET_SAMPLE_RATE,
                1,
            ))));
        }

        Ok(None)
    }

    fn drain_pending_frames(&mut self) -> Result<(), crate::error::AudioError> {
        let tail = self.resampler.flush()?;
        self.pending.extend_from_slice(&tail);
        while self.pending.len() >= self.frame_samples {
            let frame: Vec<f32> = self.pending.drain(..self.frame_samples).collect();
            let _ = self.process_frame(&frame)?;
        }
        if self.state == VadState::Speaking && !self.pending.is_empty() {
            self.segment.extend_from_slice(&self.pending);
            self.pending.clear();
        }
        Ok(())
    }

    pub fn set_config(&mut self, config: VadConfig) {
        let engine_changed = self.config.engine != config.engine;
        self.config = config.clone();
        self.analyzer
            .set_threshold_percent(config.effective_threshold_percent());
        if engine_changed {
            self.analyzer
                .rebuild_engine(config.engine, config.effective_threshold_percent());
            self.frame_samples = frame_samples_from_ms(self.analyzer.frame_ms());
        }
    }

    fn is_voice_in_frame(&mut self, frame: &[f32]) -> bool {
        let speaking = self.state == VadState::Speaking;
        self.analyzer.is_voice(frame, speaking)
    }

    fn process_frame(&mut self, frame: &[f32]) -> Result<Option<VadEvent>, crate::error::AudioError> {
        let is_voice = self.is_voice_in_frame(frame);

        match self.state {
            VadState::Idle => {
                self.pre_buffer.push_slice(frame);
                if is_voice {
                    self.state = VadState::Speaking;
                    self.silence_samples = 0;
                    let pre = self.pre_buffer.read_last(self.pre_buffer.len());
                    self.segment = pre;
                    self.segment.extend_from_slice(frame);
                    return Ok(Some(VadEvent::SpeechStarted));
                }
            }
            VadState::Speaking => {
                self.segment.extend_from_slice(frame);
                if is_voice {
                    self.silence_samples = 0;
                } else {
                    self.silence_samples += self.frame_samples;
                    if self.end_on_silence
                        && self.silence_samples
                            >= self.config.silence_timeout_samples(TARGET_SAMPLE_RATE)
                    {
                        if self.segment.len() >= self.config.minimum_speech_samples(TARGET_SAMPLE_RATE) {
                            let segment = self.take_segment()?;
                            self.state = VadState::Idle;
                            self.mark_ptt_subsegment_delivered();
                            return Ok(Some(VadEvent::SpeechEnded(segment)));
                        }
                        self.reset();
                    }
                }

                if self.segment.len() >= self.config.maximum_segment_samples(TARGET_SAMPLE_RATE) {
                    let segment = self.take_segment()?;
                    self.mark_ptt_subsegment_delivered();
                    return Ok(Some(VadEvent::SpeechEnded(segment)));
                }
            }
        }

        Ok(None)
    }

    pub fn is_speaking(&self) -> bool {
        self.state == VadState::Speaking
    }

    fn take_segment(&mut self) -> Result<AudioSegment, crate::error::AudioError> {
        let samples = std::mem::take(&mut self.segment);
        self.silence_samples = 0;
        self.pre_buffer.clear();
        Ok(AudioSegment::new(samples, TARGET_SAMPLE_RATE, 1))
    }
}

fn frame_samples_from_ms(frame_ms: u32) -> usize {
    (TARGET_SAMPLE_RATE as usize * frame_ms as usize) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{VadEngine, VadThresholdMode};

    fn test_config() -> VadConfig {
        VadConfig {
            engine: VadEngine::WebRtc,
            ..Default::default()
        }
    }

    fn frame_len() -> usize {
        WEBRTC_FRAME_SAMPLES
    }

    fn sine_frame(amplitude: f32) -> Vec<f32> {
        (0..frame_len())
            .map(|i| {
                let t = i as f32 / TARGET_SAMPLE_RATE as f32;
                amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect()
    }

    fn silence_frame() -> Vec<f32> {
        vec![0.0; frame_len()]
    }

    #[test]
    fn silence_does_not_emit_segment() {
        let mut detector = VadDetector::new(test_config(), TARGET_SAMPLE_RATE, 1);
        for _ in 0..20 {
            let events = detector.push_samples(&silence_frame()).unwrap();
            assert!(events.is_empty());
        }
    }

    #[test]
    fn silence_does_not_end_when_disabled() {
        let mut detector = VadDetector::new(
            VadConfig {
                engine: VadEngine::WebRtc,
                minimum_speech_ms: 100,
                silence_timeout_ms: 100,
                ..Default::default()
            },
            TARGET_SAMPLE_RATE,
            1,
        );
        detector.set_end_on_silence(false);

        let _ = detector.push_samples(&sine_frame(0.8)).unwrap();
        for _ in 0..20 {
            let events = detector.push_samples(&silence_frame()).unwrap();
            assert!(events.iter().all(|event| !matches!(event, VadEvent::SpeechEnded(_))));
        }
    }

    #[test]
    fn speech_start_and_end_transitions() {
        let mut detector = VadDetector::new(
            VadConfig {
                engine: VadEngine::WebRtc,
                minimum_speech_ms: 100,
                silence_timeout_ms: 100,
                ..Default::default()
            },
            TARGET_SAMPLE_RATE,
            1,
        );

        let mut saw_start = false;
        let mut saw_end = false;

        for _ in 0..15 {
            for event in detector.push_samples(&sine_frame(0.6)).unwrap() {
                if matches!(event, VadEvent::SpeechStarted) {
                    saw_start = true;
                }
            }
        }

        for _ in 0..20 {
            for event in detector.push_samples(&silence_frame()).unwrap() {
                if matches!(event, VadEvent::SpeechEnded(_)) {
                    saw_end = true;
                }
            }
        }

        assert!(saw_start);
        assert!(saw_end);
    }

    #[test]
    fn ptt_flush_after_subsegment_does_not_replay_full_capture() {
        let mut detector = VadDetector::new(
            VadConfig {
                engine: VadEngine::WebRtc,
                minimum_speech_ms: 100,
                silence_timeout_ms: 100,
                ..Default::default()
            },
            TARGET_SAMPLE_RATE,
            1,
        );
        detector.set_ptt_recording(true);
        detector.set_end_on_silence(true);

        let mut mid_chunk = false;
        for _ in 0..15 {
            let _ = detector.push_samples(&sine_frame(0.6)).unwrap();
        }
        for _ in 0..20 {
            for event in detector.push_samples(&silence_frame()).unwrap() {
                if matches!(event, VadEvent::SpeechEnded(_)) {
                    mid_chunk = true;
                }
            }
        }

        assert!(mid_chunk, "silence during PTT should deliver a mid-session chunk");

        let flush = detector.flush_ptt().unwrap();
        assert!(
            flush.is_none(),
            "release flush must not resend the whole ptt_capture after mid chunks"
        );
    }

    #[test]
    fn ptt_flush_returns_full_capture_while_recording() {
        let mut detector = VadDetector::new(test_config(), TARGET_SAMPLE_RATE, 1);
        detector.set_ptt_recording(true);
        detector.set_end_on_silence(false);

        for _ in 0..25 {
            let _ = detector.push_samples(&sine_frame(0.5)).unwrap();
        }

        let segment = match detector.flush_ptt().unwrap() {
            Some(VadEvent::SpeechEnded(segment)) => segment,
            _ => panic!("ptt flush should return audio"),
        };
        assert!(segment.duration_ms >= 400);
        assert!(segment.samples.iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn peak_gate_blocks_quiet_frames() {
        let mut detector = VadDetector::new(
            VadConfig {
                engine: VadEngine::WebRtc,
                threshold_mode: VadThresholdMode::Manual,
                voice_threshold_percent: 95,
                ..Default::default()
            },
            TARGET_SAMPLE_RATE,
            1,
        );

        for _ in 0..20 {
            let events = detector.push_samples(&sine_frame(0.2)).unwrap();
            assert!(
                events
                    .iter()
                    .all(|event| !matches!(event, VadEvent::SpeechStarted)),
                "peak gate should block frames below the manual threshold"
            );
        }
    }

    #[test]
    fn long_speech_splits_at_maximum_segment() {
        let mut detector = VadDetector::new(
            VadConfig {
                engine: VadEngine::WebRtc,
                minimum_speech_ms: 100,
                maximum_segment_ms: 500,
                ..Default::default()
            },
            TARGET_SAMPLE_RATE,
            1,
        );

        let mut chunk_count = 0;
        for _ in 0..40 {
            for event in detector.push_samples(&sine_frame(0.6)).unwrap() {
                if matches!(event, VadEvent::SpeechEnded(_)) {
                    chunk_count += 1;
                }
            }
        }

        assert!(chunk_count >= 1, "expected at least one max-chunk split");
        assert!(
            detector.is_speaking(),
            "mid-utterance split should keep the session active"
        );
    }

    #[cfg(feature = "vad-silero")]
    #[test]
    fn silero_accepts_loud_sine() {
        let mut detector = VadDetector::new(
            VadConfig {
                engine: VadEngine::Silero,
                voice_threshold_percent: 3,
                threshold_mode: VadThresholdMode::Manual,
                minimum_speech_ms: 100,
                silence_timeout_ms: 200,
                ..Default::default()
            },
            TARGET_SAMPLE_RATE,
            1,
        );
        if !crate::vad::silero_runtime_available() {
            return;
        }
        let frame_len = detector.frame_samples();
        let mut started = false;
        let mut sample_offset = 0usize;
        for _ in 0..30 {
            let loud: Vec<f32> = (0..frame_len)
                .map(|i| {
                    let n = sample_offset + i;
                    let t = n as f32 / TARGET_SAMPLE_RATE as f32;
                    let v = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.4
                        + (2.0 * std::f32::consts::PI * 700.0 * t).sin() * 0.25
                        + (2.0 * std::f32::consts::PI * 1900.0 * t).sin() * 0.1;
                    v.clamp(-1.0, 1.0)
                })
                .collect();
            sample_offset += frame_len;
            for event in detector.push_samples(&loud).unwrap() {
                if matches!(event, VadEvent::SpeechStarted) {
                    started = true;
                }
            }
        }
        assert!(
            started,
            "Silero should detect voiced multi-tone stream as speech"
        );
    }
}
