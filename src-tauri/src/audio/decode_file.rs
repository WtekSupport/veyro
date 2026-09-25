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

use crate::audio::encode::decode_wav_file;
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
                    ((self.read_bytes.saturating_mul(100)) / self.total_bytes).min(99) as u8;
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

fn decode_with_symphonia(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    let mut reader = ProgressReader::open(path, on_progress.clone())?;
    reader.report(0);
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
    let channels = track
        .codec_params
        .channels
        .map(|channels| channels.count() as u16)
        .unwrap_or(1);

    let mut decoder = codec_registry()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| error.to_string())?;

    let mut samples: Vec<f32> = Vec::new();

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
        append_decoded_buffer(&mut samples, decoded)?;
    }

    if samples.is_empty() {
        return Err("empty_audio".to_string());
    }

    if let Some(callback) = on_progress {
        callback(100);
    }

    Ok(AudioSegment::new(samples, sample_rate, channels))
}

pub fn decode_audio_file(path: &Path) -> Result<AudioSegment, String> {
    decode_audio_file_with_progress(path, None)
}

pub fn decode_audio_file_with_progress(
    path: &Path,
    on_progress: Option<DecodeProgressCallback>,
) -> Result<AudioSegment, String> {
    let segment = if is_wav_path(path) {
        if let Some(callback) = &on_progress {
            callback(0);
        }
        let segment = decode_wav_file(path)
            .or_else(|_| decode_with_symphonia(path, on_progress.clone()))?;
        if let Some(callback) = &on_progress {
            callback(100);
        }
        segment
    } else {
        decode_with_symphonia(path, on_progress.clone())?
    };

    if segment.is_empty() {
        return Err("empty_audio".to_string());
    }

    if let Some(callback) = &on_progress {
        callback(100);
    }

    resample_to_stt(segment).map_err(|error| error.to_string())
}
