
pub fn calculate_db(samples: &[f32]) -> f32 {
    if samples.is_empty() { return -100.0; }

    // 1. Square all samples and sum them
    let sq_sum: f32 = samples.iter().map(|&s| s * s).sum();
    
    // 2. Calculate the Mean and take the Square Root (RMS)
    let rms = (sq_sum / samples.len() as f32).sqrt();

    // 3. Convert to dB (with a floor of -100.0 to avoid infinity)
    if rms > 0.00001 {
        20.0 * rms.log10()
    } else {
        -100.0
    }
}
