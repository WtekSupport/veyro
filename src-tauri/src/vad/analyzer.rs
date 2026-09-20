#[cfg(feature = "vad-silero")]
use std::sync::atomic::{AtomicBool, Ordering};

use webrtc_vad::{Vad, VadMode};

use crate::settings::VadEngine;
use crate::vad::threshold::peak_level_percent;

#[cfg(feature = "vad-silero")]
pub static SILERO_RUNTIME_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Peak below this (in idle) skips Silero when WebRTC is also silent.
#[cfg(feature = "vad-silero")]
const IDLE_SILENCE_FLOOR_PERCENT: u8 = 3;

pub struct VoiceAnalyzer {
    engine: VadEngine,
    threshold_percent: u8,
    webrtc: Vad,
    #[cfg(feature = "vad-silero")]
    silero: Option<wavekat_vad::FrameAdapter>,
}

impl VoiceAnalyzer {
    pub fn new(engine: VadEngine, threshold_percent: u8) -> Self {
        let effective = effective_engine(engine);
        #[cfg(feature = "vad-silero")]
        let silero = if effective == VadEngine::Silero {
            match wavekat_vad::backends::silero::SileroVad::new(16_000) {
                Ok(vad) => {
                    SILERO_RUNTIME_AVAILABLE.store(true, Ordering::Relaxed);
                    Some(wavekat_vad::FrameAdapter::new(Box::new(vad)))
                }
                Err(error) => {
                    tracing::warn!("Silero VAD init failed, using WebRTC only: {error}");
                    SILERO_RUNTIME_AVAILABLE.store(false, Ordering::Relaxed);
                    None
                }
            }
        } else {
            SILERO_RUNTIME_AVAILABLE.store(false, Ordering::Relaxed);
            None
        };

        Self {
            engine: effective,
            threshold_percent,
            webrtc: Vad::new_with_rate_and_mode(webrtc_vad::SampleRate::Rate16kHz, VadMode::Quality),
            #[cfg(feature = "vad-silero")]
            silero,
        }
    }

    pub fn frame_ms(&self) -> u32 {
        if self.uses_silero_frames() {
            32
        } else {
            20
        }
    }

    pub fn uses_silero_frames(&self) -> bool {
        #[cfg(feature = "vad-silero")]
        {
            self.engine == VadEngine::Silero && self.silero.is_some()
        }
        #[cfg(not(feature = "vad-silero"))]
        {
            false
        }
    }

    pub fn configured_engine(&self) -> VadEngine {
        self.engine
    }

    pub fn set_threshold_percent(&mut self, threshold_percent: u8) {
        self.threshold_percent = threshold_percent;
    }

    pub fn rebuild_engine(&mut self, engine: VadEngine, threshold_percent: u8) {
        *self = Self::new(engine, threshold_percent);
    }

    pub fn is_voice(&mut self, frame: &[f32], _speaking: bool) -> bool {
        let peak = peak_level_percent(frame);
        let threshold = self.threshold_percent;
        if peak < threshold {
            return false;
        }

        let pcm16 = f32_frame_to_i16(frame);
        let webrtc_voice = self.webrtc.is_voice_segment(&pcm16).unwrap_or(false);

        if !self.uses_silero_frames() {
            return webrtc_voice;
        }

        #[cfg(feature = "vad-silero")]
        {
            let run_silero = if _speaking {
                peak >= IDLE_SILENCE_FLOOR_PERCENT || webrtc_voice
            } else {
                webrtc_voice || peak >= IDLE_SILENCE_FLOOR_PERCENT
            };
            if !run_silero {
                return false;
            }
            let Some(adapter) = self.silero.as_mut() else {
                return webrtc_voice;
            };
            let probs = match adapter.process_all(&pcm16, 16_000) {
                Ok(values) => values,
                Err(error) => {
                    tracing::debug!("Silero process failed: {error}");
                    return webrtc_voice;
                }
            };
            let prob = probs.into_iter().fold(0.0f32, f32::max);
            let prob_threshold = threshold as f32 / 100.0;
            return prob > prob_threshold;
        }

        #[cfg(not(feature = "vad-silero"))]
        {
            webrtc_voice
        }
    }
}

pub fn effective_engine(engine: VadEngine) -> VadEngine {
    if engine == VadEngine::Silero && !cfg!(feature = "vad-silero") {
        VadEngine::WebRtc
    } else {
        engine
    }
}

pub fn silero_compiled() -> bool {
    cfg!(feature = "vad-silero")
}

pub fn silero_runtime_available() -> bool {
    #[cfg(feature = "vad-silero")]
    {
        SILERO_RUNTIME_AVAILABLE.load(Ordering::Relaxed)
    }
    #[cfg(not(feature = "vad-silero"))]
    {
        false
    }
}

fn f32_frame_to_i16(frame: &[f32]) -> Vec<i16> {
    frame
        .iter()
        .map(|&sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32) as i16
        })
        .collect()
}

#[cfg(all(test, feature = "vad-silero"))]
mod tests {
    use super::*;
    use crate::settings::VadEngine;

    fn voiced_frame(frame_len: usize, offset: usize) -> Vec<f32> {
        (0..frame_len)
            .map(|i| {
                let n = offset + i;
                let t = n as f32 / 16_000.0;
                let v = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.4
                    + (2.0 * std::f32::consts::PI * 700.0 * t).sin() * 0.25
                    + (2.0 * std::f32::consts::PI * 1900.0 * t).sin() * 0.1;
                v.clamp(-1.0, 1.0)
            })
            .collect()
    }

    #[test]
    fn silero_hybrid_scores_voiced_above_silence() {
        let mut analyzer = VoiceAnalyzer::new(VadEngine::Silero, 3);
        if !silero_runtime_available() {
            return;
        }
        assert!(analyzer.uses_silero_frames());
        let frame_len = (analyzer.frame_ms() as usize * 16_000) / 1000;
        let silence = vec![0.0f32; frame_len];
        assert!(
            !analyzer.is_voice(&silence, false),
            "silence should not pass the voice gate"
        );

        let mut offset = 0usize;
        let mut any_voice = false;
        for _ in 0..40 {
            let frame = voiced_frame(frame_len, offset);
            offset += frame_len;
            if analyzer.is_voice(&frame, false) {
                any_voice = true;
                break;
            }
        }
        assert!(
            any_voice,
            "voiced test stream should exceed a low Silero probability threshold"
        );
    }
}
