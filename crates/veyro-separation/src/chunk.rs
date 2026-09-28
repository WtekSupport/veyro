use std::sync::Arc;

use crate::error::SeparationError;

pub fn process_chunks<F>(
    mix: &[f32],
    chunk_size: usize,
    hop: usize,
    progress: &Arc<dyn Fn(u8) + Send + Sync>,
    mut infer: F,
) -> Result<Vec<f32>, SeparationError>
where
    F: FnMut(&[f32]) -> Result<Vec<f32>, SeparationError>,
{
    if mix.is_empty() {
        return Err(SeparationError::InvalidAudio("empty mix".into()));
    }
    if chunk_size == 0 || hop == 0 {
        return Err(SeparationError::InvalidAudio("invalid chunk config".into()));
    }

    let mut acc = vec![0.0f32; mix.len()];
    let mut weights = vec![0.0f32; mix.len()];
    let window = hann_window(chunk_size);
    let total_chunks = ((mix.len().saturating_sub(1)) / hop).saturating_add(1);
    let mut chunk_index = 0usize;

    let mut start = 0usize;
    while start < mix.len() {
        let end = (start + chunk_size).min(mix.len());
        let mut chunk = mix[start..end].to_vec();
        if chunk.len() < chunk_size {
            chunk.resize(chunk_size, 0.0);
        }

        let mut vocal_chunk = infer(&chunk)?;
        if vocal_chunk.len() > chunk.len() {
            vocal_chunk.truncate(chunk.len());
        } else if vocal_chunk.len() < chunk.len() {
            vocal_chunk.resize(chunk.len(), 0.0);
        }

        let effective = end - start;
        for i in 0..effective {
            let w = window[i];
            acc[start + i] += vocal_chunk[i] * w;
            weights[start + i] += w;
        }

        chunk_index += 1;
        let percent = ((chunk_index as u64 * 100) / total_chunks as u64).min(100) as u8;
        progress(percent);

        if end >= mix.len() {
            break;
        }
        start = start.saturating_add(hop);
    }

    for (sample, weight) in acc.iter_mut().zip(weights.iter()) {
        if *weight > 1e-6 {
            *sample /= weight;
        }
    }
    acc.truncate(mix.len());
    Ok(acc)
}

fn hann_window(size: usize) -> Vec<f32> {
    if size <= 1 {
        return vec![1.0; size.max(1)];
    }
    (0..size)
        .map(|i| {
            let x = std::f32::consts::PI * 2.0 * i as f32 / (size as f32 - 1.0);
            0.5 * (1.0 - x.cos())
        })
        .collect()
}

#[cfg(test)]
mod overlap_add_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn overlap_add_reconstructs_constant_signal() {
        let signal = vec![1.0f32; 8000];
        let progress: Arc<dyn Fn(u8) + Send + Sync> = Arc::new(|_| {});
        let out = process_chunks(&signal, 2048, 1024, &progress, |chunk| Ok(chunk.to_vec()))
            .expect("overlap add");
        assert_eq!(out.len(), signal.len());
        for (left, right) in out.iter().zip(signal.iter()) {
            assert!((left - right).abs() < 0.05, "expected ~1.0 got {left}");
        }
    }
}
