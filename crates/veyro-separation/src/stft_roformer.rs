use std::f32::consts::PI;
use std::sync::Arc;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{Tensor, ValueType};
use realfft::num_complex::Complex;
use realfft::RealFftPlanner;

use crate::error::SeparationError;
use crate::model::LoadedModel;
use crate::session::ExecutionProvider;

const N_FFT: usize = 2048;
const HOP: usize = 441;
/// Fallback when the ONNX graph marks the time axis as dynamic (`-1`).
const DEFAULT_N_TIME: usize = 1100;
const N_FREQ: usize = N_FFT / 2 + 1;
const PACKED: usize = N_FREQ * 2;
const PAD: usize = N_FFT / 2;

fn chunk_mono(n_time: usize) -> usize {
    HOP * (n_time.saturating_sub(1))
}

struct StftSession {
    inner: Session,
    n_time: usize,
}

impl StftSession {
    fn open(model: &LoadedModel, _provider: ExecutionProvider) -> Result<Self, SeparationError> {
        // This fp16 RoFormer core was validated on CPU. DirectML/CUDA can "succeed"
        // while emitting near-constant masks → vocals ≈ instrumental ≈ 0.5·mix.
        let mut builder = Session::builder().map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        builder = builder
            .with_execution_providers([ort::ep::CPU::default().build()])
            .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        let inner = builder
            .commit_from_file(model.path())
            .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;

        let input = inner
            .inputs()
            .first()
            .ok_or_else(|| SeparationError::ModelLoad("model has no inputs".into()))?;
        let n_time = parse_n_time(input.dtype())?;
        tracing::info!(
            input = %input.name(),
            n_time,
            path = %model.path().display(),
            "loaded STFT RoFormer session (CPU)"
        );
        Ok(Self { inner, n_time })
    }

    fn run_masks(&mut self, stft_repr: &[f32]) -> Result<Vec<f32>, SeparationError> {
        let expected = PACKED * self.n_time * 2;
        if stft_repr.len() != expected {
            return Err(SeparationError::Inference(format!(
                "stft_repr length {} != expected {} (n_time={})",
                stft_repr.len(),
                expected,
                self.n_time
            )));
        }
        let shape = [1_i64, PACKED as i64, self.n_time as i64, 2];
        let tensor = Tensor::from_array((shape, stft_repr.to_vec()))
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        let outputs = self
            .inner
            .run(ort::inputs!["stft_repr" => tensor])
            .map_err(|e| SeparationError::Inference(e.to_string()))?;

        let mut first = None;
        let mut masks = None;
        for (name, value) in outputs {
            if name == "masks" {
                masks = Some(value);
                break;
            }
            if first.is_none() {
                first = Some(value);
            }
        }
        let output = masks
            .or(first)
            .ok_or_else(|| SeparationError::Inference("empty model output".into()))?;
        let (_, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        if data.len() != stft_repr.len() {
            return Err(SeparationError::Inference(format!(
                "mask length {} != stft length {}",
                data.len(),
                stft_repr.len()
            )));
        }
        validate_masks(data)?;
        Ok(data.to_vec())
    }
}

fn parse_n_time(dtype: &ValueType) -> Result<usize, SeparationError> {
    let ValueType::Tensor { shape, .. } = dtype else {
        return Err(SeparationError::ModelLoad(
            "stft_repr input is not a tensor".into(),
        ));
    };
    if shape.len() != 4 {
        return Err(SeparationError::ModelLoad(format!(
            "stft_repr shape rank {} != 4 ({shape:?})",
            shape.len()
        )));
    }
    let packed = shape[1];
    let time = shape[2];
    let complex = shape[3];
    if packed > 0 && packed as usize != PACKED {
        return Err(SeparationError::ModelLoad(format!(
            "stft_repr packed dim {packed} != expected {PACKED}"
        )));
    }
    if complex > 0 && complex != 2 {
        return Err(SeparationError::ModelLoad(format!(
            "stft_repr complex dim {complex} != 2"
        )));
    }
    let n_time = if time > 0 {
        time as usize
    } else {
        DEFAULT_N_TIME
    };
    if n_time < 2 {
        return Err(SeparationError::ModelLoad(format!(
            "stft_repr time dim too small: {n_time}"
        )));
    }
    Ok(n_time)
}

fn validate_masks(data: &[f32]) -> Result<(), SeparationError> {
    let pairs = data.len() / 2;
    if pairs == 0 {
        return Err(SeparationError::Inference("empty masks".into()));
    }
    let mut sum_re = 0.0f64;
    let mut sum_im = 0.0f64;
    let mut sum_mag = 0.0f64;
    let mut sum_mag_sq = 0.0f64;
    let mut nan_or_inf = 0usize;
    for i in (0..data.len()).step_by(2) {
        let re = data[i];
        let im = data[i + 1];
        if !re.is_finite() || !im.is_finite() {
            nan_or_inf += 1;
            continue;
        }
        let mag = (re * re + im * im).sqrt() as f64;
        sum_re += re as f64;
        sum_im += im as f64;
        sum_mag += mag;
        sum_mag_sq += mag * mag;
    }
    if nan_or_inf > 0 {
        return Err(SeparationError::Inference(format!(
            "RoFormer masks contain {nan_or_inf} non-finite values (bad EP / weights?)"
        )));
    }
    let n = pairs as f64;
    let mean_mag = sum_mag / n;
    let var_mag = (sum_mag_sq / n) - mean_mag * mean_mag;
    let mean_re = sum_re / n;
    let mean_im = sum_im / n;
    // Constant real gain (e.g. ~0.5) makes vocals == instrumental after residual.
    if var_mag < 1e-6 && mean_im.abs() < 1e-3 && (0.05..0.95).contains(&mean_re) {
        return Err(SeparationError::Inference(format!(
            "RoFormer masks collapsed to ~constant gain (re={mean_re:.3}, |im|={:.3}, var={var_mag:.2e}); \
             check that syhft_core_t1100.onnx.data sits next to the .onnx",
            mean_im.abs()
        )));
    }
    tracing::debug!(
        mean_mag,
        var_mag,
        mean_re,
        mean_im,
        "RoFormer mask stats"
    );
    Ok(())
}

pub fn separate_stft_roformer(
    model: &LoadedModel,
    mix: &crate::buffer::AudioBuffer,
    provider: ExecutionProvider,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(crate::buffer::AudioBuffer, crate::buffer::AudioBuffer), SeparationError> {
    if mix.sample_rate != 44_100 {
        return Err(SeparationError::InvalidAudio(
            "STFT RoFormer requires 44.1 kHz stereo input".into(),
        ));
    }
    if mix.channels != 2 {
        return Err(SeparationError::InvalidAudio(
            "STFT RoFormer requires stereo input".into(),
        ));
    }

    let frames = mix.samples.len() / 2;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for frame in 0..frames {
        left.push(mix.samples[frame * 2]);
        right.push(mix.samples[frame * 2 + 1]);
    }

    let mut session = StftSession::open(model, provider)?;
    let n_time = session.n_time;
    let chunk_len = chunk_mono(n_time);
    let track_step = model.profile().stft_track_step_samples();
    let chunk_window = hamming(chunk_len);
    let mut vocal_left = vec![0.0f32; frames];
    let mut vocal_right = vec![0.0f32; frames];
    let mut weight = vec![0.0f32; frames];

    let mut starts = Vec::new();
    let mut start = 0usize;
    while start < frames {
        starts.push(start);
        if start + chunk_len >= frames {
            break;
        }
        start += track_step;
    }
    let total = starts.len().max(1);

    for (index, chunk_start) in starts.into_iter().enumerate() {
        let chunk_end = (chunk_start + chunk_len).min(frames);
        let mut chunk_l = left[chunk_start..chunk_end].to_vec();
        let mut chunk_r = right[chunk_start..chunk_end].to_vec();
        if chunk_l.len() < chunk_len {
            chunk_l.resize(chunk_len, 0.0);
            chunk_r.resize(chunk_len, 0.0);
        }

        let stft = encode_stft_repr(&chunk_l, &chunk_r, n_time)?;
        let masks = session.run_masks(&stft)?;
        let masked = apply_complex_masks(&stft, &masks);
        let (out_l, out_r) = decode_stft_to_stereo(&masked, n_time)?;

        let effective = chunk_end - chunk_start;
        for i in 0..effective {
            let w = chunk_window[i];
            vocal_left[chunk_start + i] += out_l[i] * w;
            vocal_right[chunk_start + i] += out_r[i] * w;
            weight[chunk_start + i] += w;
        }

        let percent = (((index + 1) as u64 * 100) / total as u64).min(100) as u8;
        progress(percent);
    }

    for i in 0..frames {
        if weight[i] > 1e-6 {
            vocal_left[i] /= weight[i];
            vocal_right[i] /= weight[i];
        }
    }

    let mut vocals = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        vocals.push(vocal_left[i]);
        vocals.push(vocal_right[i]);
    }

    let instrumental = subtract_stems(&mix.samples, &vocals);
    Ok((
        crate::buffer::AudioBuffer::new(vocals, mix.sample_rate, 2),
        crate::buffer::AudioBuffer::new(instrumental, mix.sample_rate, 2),
    ))
}

fn subtract_stems(mix: &[f32], vocals: &[f32]) -> Vec<f32> {
    let len = mix.len().min(vocals.len());
    mix.iter()
        .zip(vocals.iter())
        .take(len)
        .map(|(m, v)| m - v)
        .collect()
}

fn hann_periodic(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            // Match torch.hann_window(..., periodic=True) / musetric WGSL.
            let phase = 2.0 * PI * i as f32 / size as f32;
            0.5 - 0.5 * phase.cos()
        })
        .collect()
}

fn hamming(size: usize) -> Vec<f32> {
    if size <= 1 {
        return vec![1.0; size.max(1)];
    }
    (0..size)
        .map(|i| {
            let phase = 2.0 * PI * i as f32 / (size as f32 - 1.0);
            0.54 - 0.46 * phase.cos()
        })
        .collect()
}

/// Torch / musetric center reflect: index `i` into length `samples` with pad.
fn reflect_index(index: i32, samples: i32) -> usize {
    if index < 0 {
        return (-index) as usize;
    }
    if index >= samples {
        return (2 * samples - 2 - index) as usize;
    }
    index as usize
}

fn encode_stft_repr(
    left: &[f32],
    right: &[f32],
    n_time: usize,
) -> Result<Vec<f32>, SeparationError> {
    let chunk_len = chunk_mono(n_time);
    if left.len() != right.len() || left.len() != chunk_len {
        return Err(SeparationError::InvalidAudio(format!(
            "expected {chunk_len} samples per channel (n_time={n_time}), got {}",
            left.len()
        )));
    }

    let window = hann_periodic(N_FFT);
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);
    let mut scratch = fft.make_scratch_vec();
    let mut time_buf = vec![0.0f32; N_FFT];
    let mut freq_buf = fft.make_output_vec();
    let samples = chunk_len as i32;
    let pad = PAD as i32;

    let mut tensor = vec![0.0f32; PACKED * n_time * 2];

    for frame in 0..n_time {
        for ch in 0..2 {
            let channel = if ch == 0 { left } else { right };
            for n in 0..N_FFT {
                let idx = reflect_index(frame as i32 * HOP as i32 + n as i32 - pad, samples);
                time_buf[n] = channel[idx] * window[n];
            }
            fft.process_with_scratch(&mut time_buf, &mut freq_buf, &mut scratch)
                .map_err(|e| SeparationError::Inference(e.to_string()))?;
            for freq in 0..N_FREQ {
                // packed = 2*freq + channel (musetric pack.wgsl)
                let packed = 2 * freq + ch;
                let c = &freq_buf[freq];
                let base = (packed * n_time + frame) * 2;
                tensor[base] = c.re;
                tensor[base + 1] = c.im;
            }
        }
    }
    Ok(tensor)
}

fn apply_complex_masks(stft: &[f32], masks: &[f32]) -> Vec<f32> {
    let len = stft.len().min(masks.len());
    let mut out = vec![0.0f32; len];
    for i in (0..len).step_by(2) {
        let re_a = stft[i];
        let im_a = stft.get(i + 1).copied().unwrap_or(0.0);
        let re_b = masks[i];
        let im_b = masks.get(i + 1).copied().unwrap_or(0.0);
        out[i] = re_a * re_b - im_a * im_b;
        out[i + 1] = re_a * im_b + im_a * re_b;
    }
    out
}

fn decode_stft_to_stereo(
    masked: &[f32],
    n_time: usize,
) -> Result<(Vec<f32>, Vec<f32>), SeparationError> {
    let chunk_len = chunk_mono(n_time);
    let window = hann_periodic(N_FFT);
    let mut planner = RealFftPlanner::<f32>::new();
    let ifft = planner.plan_fft_inverse(N_FFT);
    let mut scratch = ifft.make_scratch_vec();
    let mut time_buf = ifft.make_output_vec();
    let mut freq_buf = ifft.make_input_vec();

    // Per-frame time domain: [channel][frame][n]
    let mut frame_time = vec![0.0f32; 2 * n_time * N_FFT];

    for frame in 0..n_time {
        for ch in 0..2 {
            for freq in 0..N_FREQ {
                let packed = 2 * freq + ch;
                let base = (packed * n_time + frame) * 2;
                freq_buf[freq] = Complex {
                    re: masked[base],
                    im: masked[base + 1],
                };
            }
            enforce_real_fft_spectrum(&mut freq_buf);
            ifft.process_with_scratch(&mut freq_buf, &mut time_buf, &mut scratch)
                .map_err(|e| SeparationError::Inference(e.to_string()))?;
            // realfft inverse is unnormalized (×N); musetric/torch ISTFT expect 1/N.
            let norm = 1.0 / N_FFT as f32;
            let dest = (ch * n_time + frame) * N_FFT;
            for i in 0..N_FFT {
                frame_time[dest + i] = time_buf[i] * norm;
            }
        }
    }

    // musetric overlapAdd: output sample `s` reads padded position `s + pad`.
    let mut left = vec![0.0f32; chunk_len];
    let mut right = vec![0.0f32; chunk_len];
    let pad = PAD as i32;
    let hop = HOP as i32;
    let n_fft = N_FFT as i32;
    let frames = n_time as i32;

    for sample in 0..chunk_len {
        let padded = sample as i32 + pad;
        let mut first_frame = (padded - n_fft + 1).div_euclid(hop);
        let mut last_frame = padded.div_euclid(hop);
        first_frame = first_frame.max(0);
        last_frame = last_frame.min(frames - 1);
        if first_frame > last_frame {
            continue;
        }
        for ch in 0..2 {
            let mut sum = 0.0f32;
            let mut envelope = 0.0f32;
            for frame in first_frame..=last_frame {
                let n = (padded - frame * hop) as usize;
                if n >= N_FFT {
                    continue;
                }
                let w = window[n];
                let src = (ch * n_time + frame as usize) * N_FFT + n;
                sum += frame_time[src] * w;
                envelope += w * w;
            }
            let value = if envelope > 1e-8 { sum / envelope } else { 0.0 };
            if ch == 0 {
                left[sample] = value;
            } else {
                right[sample] = value;
            }
        }
    }
    Ok((left, right))
}

/// Real inverse FFT requires Im=0 at DC and Nyquist; complex masks can break that.
fn enforce_real_fft_spectrum(freq_buf: &mut [Complex<f32>]) {
    if let Some(dc) = freq_buf.first_mut() {
        dc.im = 0.0;
    }
    if freq_buf.len() > 1 {
        if let Some(nyquist) = freq_buf.last_mut() {
            nyquist.im = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_mask_roundtrip_keeps_energy() {
        let n_time = DEFAULT_N_TIME;
        let chunk_len = chunk_mono(n_time);
        let mut left = vec![0.0f32; chunk_len];
        let mut right = vec![0.0f32; chunk_len];
        for i in 0..chunk_len {
            let t = i as f32 / 44100.0;
            left[i] = (2.0 * PI * 440.0 * t).sin() * 0.5;
            right[i] = (2.0 * PI * 660.0 * t).sin() * 0.4;
        }
        let stft = encode_stft_repr(&left, &right, n_time).expect("stft");
        let mut identity = vec![0.0f32; stft.len()];
        for i in (0..identity.len()).step_by(2) {
            identity[i] = 1.0; // re
            identity[i + 1] = 0.0; // im
        }
        let masked = apply_complex_masks(&stft, &identity);
        let (out_l, out_r) = decode_stft_to_stereo(&masked, n_time).expect("istft");

        let mid = chunk_len / 2;
        let span = 2048;
        let mut err = 0.0f32;
        let mut energy = 0.0f32;
        for i in (mid - span)..(mid + span) {
            err += (out_l[i] - left[i]).powi(2) + (out_r[i] - right[i]).powi(2);
            energy += left[i].powi(2) + right[i].powi(2);
        }
        let snr = 10.0 * (energy / err.max(1e-12)).log10();
        assert!(
            snr > 20.0,
            "identity STFT roundtrip SNR too low: {snr:.1} dB (center-trim / OLA bug?)"
        );
    }
}
