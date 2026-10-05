use crate::audio::segment::AudioSegment;
use crate::settings::{AppSettings, SherpaOnnxLayout};

/// Exported NeMo/GigaAM encoders in sherpa-onnx fail when feature length exceeds ~5000
/// (ONNX broadcast error, e.g. 5000 vs 6279 in self-attention). ~48s at 10ms features is safe.
pub const NEMO_ENCODER_MAX_AUDIO_MS: u64 = 48_000;
/// Overlap between consecutive decode chunks to align text at boundaries.
pub const NEMO_ENCODER_CHUNK_OVERLAP_MS: u64 = 2_000;

pub fn max_offline_audio_ms(settings: &AppSettings) -> u64 {
    let layout = crate::settings::variant_spec(settings.local_stt_variant()).sherpa_layout;
    match layout {
        Some(
            SherpaOnnxLayout::GigaAmTransducerInt8
            | SherpaOnnxLayout::NemoCtcInt8
            | SherpaOnnxLayout::NemoInt8
            | SherpaOnnxLayout::NemoFpOnnx,
        ) => NEMO_ENCODER_MAX_AUDIO_MS,
        Some(SherpaOnnxLayout::Qwen3Int8) => 30_000,
        None => u64::MAX,
    }
}

pub fn clip_audio_for_offline_decode(audio: &AudioSegment, max_ms: u64) -> Vec<AudioSegment> {
    let max_ms = max_ms.max(1_000);
    if audio.duration_ms <= max_ms {
        return vec![audio.clone()];
    }
    let overlap = NEMO_ENCODER_CHUNK_OVERLAP_MS.min(max_ms / 4);
    let mut out = Vec::new();
    let mut offset_ms = 0u64;
    while offset_ms < audio.duration_ms {
        let end_ms = (offset_ms + max_ms).min(audio.duration_ms);
        let chunk = audio.clip_ms(offset_ms, end_ms);
        if !chunk.is_empty() {
            out.push(chunk);
        }
        if end_ms >= audio.duration_ms {
            break;
        }
        offset_ms = end_ms.saturating_sub(overlap);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::segment::AudioSegment;

    #[test]
    fn splits_long_audio_into_chunks() {
        let rate = 16_000u32;
        let samples = vec![0.01f32; rate as usize * 63];
        let audio = AudioSegment::new(samples, rate, 1);
        let chunks = clip_audio_for_offline_decode(&audio, 48_000);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.duration_ms <= 48_000));
    }
}
