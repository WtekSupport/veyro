pub fn peak_normalize(buffer: &mut crate::buffer::AudioBuffer) {
    let peak = buffer
        .samples
        .iter()
        .copied()
        .fold(0.0f32, |acc, v| acc.max(v.abs()));
    if peak <= 1e-6 {
        return;
    }
    let scale = 0.99 / peak;
    for sample in &mut buffer.samples {
        *sample *= scale;
    }
}
