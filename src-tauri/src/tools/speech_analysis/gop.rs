/// Token-level GOP proxy from alignment match quality (CTC log-probs when available).
pub fn gop_from_match(matched: bool) -> f32 {
    if matched {
        0.0
    } else {
        -1.0
    }
}

pub fn mean_gop(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f32>() / values.len() as f32)
}

pub fn count_below_threshold(values: &[f32], threshold: f32) -> usize {
    values.iter().filter(|value| **value < threshold).count()
}
