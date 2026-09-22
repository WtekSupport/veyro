use std::fs;
use std::io::Cursor;
use std::path::Path;

use flacenc::bitsink::ByteSink;
use flacenc::component::BitRepr;
use flacenc::config::Encoder;
use flacenc::encode_with_fixed_block_size;
use flacenc::error::Verify;
use flacenc::source::MemSource;
use hound::{WavSpec, WavWriter};

use crate::audio::segment::AudioSegment;

pub fn samples_to_i16(segment: &AudioSegment) -> Vec<i16> {
    segment
        .samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

pub fn encode_wav(segment: &AudioSegment) -> Result<Vec<u8>, String> {
    let mut buffer = Cursor::new(Vec::new());
    let spec = WavSpec {
        channels: 1,
        sample_rate: segment.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::new(&mut buffer, spec).map_err(|error| error.to_string())?;
    for sample in samples_to_i16(segment) {
        writer
            .write_sample(sample)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())?;
    Ok(buffer.into_inner())
}

pub fn decode_wav(bytes: &[u8]) -> Result<AudioSegment, String> {
    let cursor = Cursor::new(bytes);
    let mut reader = hound::WavReader::new(cursor).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| value as f32 / i16::MAX as f32))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?,
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?,
    };

    Ok(AudioSegment::new(
        samples,
        spec.sample_rate,
        spec.channels,
    ))
}

pub fn decode_wav_file(path: &Path) -> Result<AudioSegment, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    decode_wav(&bytes)
}

pub fn encode_flac(segment: &AudioSegment) -> Result<Vec<u8>, String> {
    let pcm: Vec<i32> = samples_to_i16(segment)
        .into_iter()
        .map(i32::from)
        .collect();

    let source = MemSource::from_samples(&pcm, 1, 16, segment.sample_rate as usize);
    let config = Encoder::default()
        .into_verified()
        .map_err(|error| format!("{error:?}"))?;
    let stream =
        encode_with_fixed_block_size(&config, source, 4096).map_err(|error| format!("{error:?}"))?;
    let mut sink = ByteSink::with_capacity(stream.count_bits());
    stream.write(&mut sink).map_err(|error| format!("{error:?}"))?;
    Ok(sink.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flac_encoding_produces_bytes() {
        let segment = AudioSegment::new(vec![0.0, 0.25, -0.25, 0.5], 16_000, 1);
        let encoded = encode_flac(&segment).expect("flac encode");
        assert!(!encoded.is_empty());
    }

    #[test]
    fn wav_roundtrip() {
        let segment = AudioSegment::new(vec![0.0, 0.25, -0.25, 0.5], 16_000, 1);
        let bytes = encode_wav(&segment).expect("encode");
        let decoded = decode_wav(&bytes).expect("decode");
        assert_eq!(decoded.samples.len(), segment.samples.len());
        assert_eq!(decoded.sample_rate, segment.sample_rate);
    }
}
