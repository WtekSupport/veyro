use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CodecRegistry, DecoderOptions};
use symphonia_adapter_libopus::OpusDecoder;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::encode::{decode_wav, decode_wav_file};
use crate::audio::resampler::{MonoResampler, TARGET_SAMPLE_RATE};
use crate::audio::segment::AudioSegment;
use crate::error::AudioError;

fn is_wav_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
}

fn resample_to_stt(segment: AudioSegment) -> Result<AudioSegment, AudioError> {
    if segment.sample_rate == TARGET_SAMPLE_RATE && segment.channels == 1 {
        return Ok(segment);
    }

    let mut resampler = MonoResampler::new(segment.sample_rate, TARGET_SAMPLE_RATE)?;
    let mut samples = resampler.push(&segment.samples, segment.channels)?;
    samples.extend(resampler.flush()?);
    Ok(AudioSegment::new(samples, TARGET_SAMPLE_RATE, 1))
}

fn append_decoded_buffer(samples: &mut Vec<f32>, decoded: AudioBufferRef<'_>) -> Result<(), String> {
    match decoded {
        AudioBufferRef::F32(buffer) => {
            let channels = buffer.spec().channels.count();
            for frame in 0..buffer.frames() {
                for ch in 0..channels {
                    samples.push(buffer.chan(ch)[frame]);
                }
            }
        }
        AudioBufferRef::F64(buffer) => {
            let channels = buffer.spec().channels.count();
            for frame in 0..buffer.frames() {
                for ch in 0..channels {
                    samples.push(buffer.chan(ch)[frame] as f32);
                }
            }
        }
        AudioBufferRef::S16(buffer) => {
            let channels = buffer.spec().channels.count();
            for frame in 0..buffer.frames() {
                for ch in 0..channels {
                    samples.push(buffer.chan(ch)[frame] as f32 / i16::MAX as f32);
                }
            }
        }
        AudioBufferRef::S32(buffer) => {
            let channels = buffer.spec().channels.count();
            for frame in 0..buffer.frames() {
                for ch in 0..channels {
                    samples.push(buffer.chan(ch)[frame] as f32 / i32::MAX as f32);
                }
            }
        }
        AudioBufferRef::U8(buffer) => {
            let channels = buffer.spec().channels.count();
            for frame in 0..buffer.frames() {
                for ch in 0..channels {
                    samples.push((buffer.chan(ch)[frame] as f32 - 128.0) / 128.0);
                }
            }
        }
        _ => return Err("unsupported_sample_format".to_string()),
    }
    Ok(())
}

fn codec_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_all::<OpusDecoder>();
    registry
}

pub type DecodeProgressCallback = Arc<dyn Fn(u8) + Send + Sync + 'static>;

struct ProgressReader {
    inner: File,
    total_bytes: u64,
    read_bytes: u64,
    last_reported: u8,
    on_progress: Option<DecodeProgressCallback>,
}

impl ProgressReader {
    fn open(path: &Path, on_progress: Option<DecodeProgressCallback>) -> Result<Self, String> {
        let inner = File::open(path).map_err(|error| error.to_string())?;
        let total_bytes = inner.metadata().map(|meta| meta.len()).unwrap_or(0);
        Ok(Self {
            inner,
            total_bytes,
            read_bytes: 0,
            last_reported: 0,
            on_progress,
        })
    }

    fn report(&mut self, percent: u8) {
        if percent <= self.last_reported && percent < 100 {
            return;
        }
        self.last_reported = percent;
        if let Some(callback) = &self.on_progress {
            callback(percent);
        }
    }

    fn sync_offset_from_file(&mut self) {
        if let Ok(position) = self.inner.stream_position() {
            self.read_bytes = position;
            if self.total_bytes > 0 {
                let percent =
                    ((self.read_bytes.saturating_mul(100)) / self.total_bytes).min(100) as u8;
                let percent = if percent >= 100 { 99 } else { percent };
                self.report(percent);
            }
        }
    }
}

impl Read for ProgressReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read > 0 {
            self.sync_offset_from_file();
        }
        Ok(read)
    }
}

impl Seek for ProgressReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let position = self.inner.seek(pos)?;
        self.read_bytes = position;
        self.sync_offset_from_file();
        Ok(position)
    }
}

impl MediaSource for ProgressReader {
    fn is_seekable(&self) -> bool {
        MediaSource::is_seekable(&self.inner)
    }

    fn byte_len(&self) -> Option<u64> {
        if self.total_bytes > 0 {
            Some(self.total_bytes)
        } else {
            MediaSource::byte_len(&self.inner)
        }
    }
}

fn report_decode_progress(on_progress: &Option<DecodeProgressCallback>, last: &mut u8, percent: u8) {
    let percent = percent.min(100);
    if percent <= *last && percent < 100 {
        return;
    }
    *last = percent;
    if let Some(callback) = on_progress {
        callback(percent);
    }
}

/// Map a local 0–100 sub-progress into `[start, end]` on the outer decode scale.
fn map_progress_span(
    parent: Option<DecodeProgressCallback>,
    start: u8,
    end: u8,
) -> Option<DecodeProgressCallback> {
    let parent = parent?;
    Some(Arc::new(move |local: u8| {
        let local = u32::from(local.min(100));
        let start = u32::from(start);
        let end = u32::from(end);
        let span = end.saturating_sub(start);
        let mapped = start.saturating_add(local.saturating_mul(span) / 100);
        parent(mapped.min(100) as u8);
    }))
}

fn decode_wav_file_with_byte_progress(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    let mut last_reported = 0u8;
    report_decode_progress(&on_progress, &mut last_reported, 0);

    let read_progress = map_progress_span(on_progress.clone(), 0, 38);
    let mut reader = ProgressReader::open(path, read_progress)?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;

    let segment = decode_wav(&bytes)?;
    report_decode_progress(&on_progress, &mut last_reported, 45);
    Ok(segment)
}

fn decode_with_symphonia(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    let mut decode_last = 0u8;
    let io_progress = map_progress_span(on_progress.clone(), 0, 22);
    let mut reader = ProgressReader::open(path, io_progress)?;
    reader.report(0);
    report_decode_progress(&on_progress, &mut decode_last, 0);
    let mss = MediaSourceStream::new(Box::new(reader), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| error.to_string())?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "no_audio_track".to_string())?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| "missing_sample_rate".to_string())?;
    // AAC/MP4 often omit channel layout in codec_params until the first packet is decoded.
    // Prefer the decoded buffer's channel count so stereo is not mislabeled as mono
    // (that would stretch duration ~2× after separation).
    let mut channels = track
        .codec_params
        .channels
        .map(|channels| channels.count() as u16)
        .unwrap_or(0);

    let mut decoder = codec_registry()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| error.to_string())?;

    let mut samples: Vec<f32> = Vec::new();
    let total_frames = track.codec_params.n_frames.filter(|frames| *frames > 0);
    let mut decoded_frames = 0u64;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::ResetRequired) => continue,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::IoError(_)) if !samples.is_empty() => break,
            Err(error) => return Err(error.to_string()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder
            .decode(&packet)
            .map_err(|error| error.to_string())?;
        let packet_channels = decoded.spec().channels.count() as u16;
        if packet_channels == 0 {
            return Err("missing_channels".to_string());
        }
        if channels == 0 {
            channels = packet_channels;
        } else if channels != packet_channels {
            return Err(format!(
                "channel count changed during decode ({channels} -> {packet_channels})"
            ));
        }
        decoded_frames = decoded_frames.saturating_add(decoded.frames() as u64);
        if let Some(total) = total_frames {
            let local = ((decoded_frames.saturating_mul(100)) / total).min(100) as u8;
            let mapped = 22u8.saturating_add(local.saturating_mul(56) / 100);
            report_decode_progress(&on_progress, &mut decode_last, mapped);
        }
        append_decoded_buffer(&mut samples, decoded)?;
    }

    if samples.is_empty() {
        return Err("empty_audio".to_string());
    }
    if channels == 0 {
        return Err("missing_channels".to_string());
    }
    if samples.len() % channels as usize != 0 {
        return Err(format!(
            "decoded sample count {} is not divisible by channel count {channels}",
            samples.len()
        ));
    }

    report_decode_progress(&on_progress, &mut decode_last, 78);

    Ok(AudioSegment::new(samples, sample_rate, channels))
}

pub fn decode_audio_file(path: &Path) -> Result<AudioSegment, String> {
    decode_audio_file_with_progress(path, None)
}

fn is_wma_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wma"))
}

fn decode_raw_segment(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    if is_wma_path(path) {
        return Err("unsupported_format".to_string());
    }

    let segment = if is_wav_path(path) {
        decode_wav_file_with_byte_progress(path, on_progress.clone())
            .or_else(|_| decode_wav_file(path))
            .or_else(|_| decode_with_symphonia(path, on_progress.clone()))?
    } else {
        decode_with_symphonia(path, on_progress.clone())?
    };

    if segment.is_empty() {
        return Err("empty_audio".to_string());
    }

    let mut last = 0u8;
    report_decode_progress(&on_progress, &mut last, 88);
    Ok(segment)
}

/// Decode to native PCM without STT resampling (preserves channel count and source rate).
pub fn decode_audio_file_raw_with_progress(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    decode_raw_segment(path, on_progress)
}

pub fn decode_audio_file_with_progress(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    let segment = decode_raw_segment(path, on_progress.clone())?;

    let mut last = 0u8;
    report_decode_progress(&on_progress, &mut last, 82);
    let resampled = resample_to_stt(segment).map_err(|error| error.to_string())?;
    report_decode_progress(&on_progress, &mut last, 88);

    Ok(resampled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::resampler::TARGET_SAMPLE_RATE;

    #[test]
    fn stt_decode_path_still_targets_16k_mono() {
        let samples: Vec<f32> = (0..4410).map(|i| (i as f32).sin() * 0.01).collect();
        let stereo = AudioSegment::new(samples, 44_100, 2);
        let resampled = resample_to_stt(stereo).expect("resample");
        assert_eq!(resampled.sample_rate, TARGET_SAMPLE_RATE);
        assert_eq!(resampled.channels, 1);
        assert!(!resampled.samples.is_empty());
    }
}
