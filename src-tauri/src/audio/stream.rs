use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crossbeam_channel::Sender;
use tracing::warn;

use crate::audio::level::{peak_level_percent, update_mic_level};
use crate::audio::monitor::MicMonitor;
use crate::error::AudioError;

use super::device::resolve_input_device;

pub struct AudioStreamHandle {
    _stream: Stream,
    stop_flag: Arc<AtomicBool>,
}

impl AudioStreamHandle {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

pub fn start_input_stream(
    device_id: Option<&str>,
    sample_tx: Sender<Vec<f32>>,
    mic_level: Arc<AtomicU32>,
    mic_monitor: Option<MicMonitor>,
) -> Result<(AudioStreamHandle, u32, u16, String), AudioError> {
    let device = resolve_input_device(device_id)?;
    let device_name = device
        .name()
        .unwrap_or_else(|_| device_id.unwrap_or("default").to_string());
    let config = device
        .default_input_config()
        .map_err(|error| AudioError::Stream(error.to_string()))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let stream_config: StreamConfig = config.clone().into();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_in_callback = stop_flag.clone();

    let stream = match config.sample_format() {
        SampleFormat::F32 => {
            build_stream::<f32>(
                &device,
                &stream_config,
                sample_tx,
                stop_in_callback,
                mic_level,
                mic_monitor.clone(),
            )?
        }
        SampleFormat::I16 => {
            build_stream::<i16>(
                &device,
                &stream_config,
                sample_tx,
                stop_in_callback,
                mic_level,
                mic_monitor.clone(),
            )?
        }
        SampleFormat::U16 => {
            build_stream::<u16>(
                &device,
                &stream_config,
                sample_tx,
                stop_in_callback,
                mic_level,
                mic_monitor,
            )?
        }
        format => return Err(AudioError::Stream(format!("unsupported sample format: {format:?}"))),
    };

    stream
        .play()
        .map_err(|error| AudioError::Stream(error.to_string()))?;

    Ok((
        AudioStreamHandle {
            _stream: stream,
            stop_flag,
        },
        sample_rate,
        channels,
        device_name,
    ))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_tx: Sender<Vec<f32>>,
    stop_flag: Arc<AtomicBool>,
    mic_level: Arc<AtomicU32>,
    mic_monitor: Option<MicMonitor>,
) -> Result<Stream, AudioError>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let err_fn = |error| warn!("audio stream error: {error}");

    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if stop_flag.load(Ordering::Relaxed) {
                    return;
                }

                let samples: Vec<f32> = data.iter().map(|s| s.to_sample()).collect();
                update_mic_level(&mic_level, &samples);
                if let Some(monitor) = &mic_monitor {
                    monitor.push_level(peak_level_percent(&samples));
                }
                if sample_tx.try_send(samples).is_err() {
                    // Drop when VAD worker is backlogged — bounded channel protects memory.
                }
            },
            err_fn,
            None,
        )
        .map_err(|error| AudioError::Stream(error.to_string()))
}
