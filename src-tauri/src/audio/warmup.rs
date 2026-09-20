use cpal::traits::DeviceTrait;
use tracing::info;

use crate::audio::device::resolve_input_device;
use crate::error::AudioError;
use crate::vad::{VadConfig, VadDetector};

pub fn query_device_format(device_id: Option<&str>) -> Result<(u32, u16), AudioError> {
    let device = resolve_input_device(device_id)?;
    let config = device
        .default_input_config()
        .map_err(|error| AudioError::Stream(error.to_string()))?;
    Ok((config.sample_rate().0, config.channels()))
}

pub fn warm_vad_detector(vad_config: VadConfig, sample_rate: u32, channels: u16) {
    std::thread::Builder::new()
        .name("vad-warmup".into())
        .spawn(move || {
            let mut detector = VadDetector::new(vad_config.clone(), sample_rate, channels);
            let frame_samples = detector.frame_samples();
            let chunk_len = frame_samples * channels as usize * 8;
            let silence = vec![0.0f32; chunk_len.max(frame_samples)];
            let _ = detector.push_samples(&silence);
            let silence = vec![0.0f32; frame_samples];
            let _ = detector.push_samples(&silence);
            info!("vad detector prewarmed ({sample_rate} Hz, {channels} ch)");
        })
        .ok();
}
