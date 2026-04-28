pub struct DbMeter {
    pub current_db: f32,
    pub release_speed: f32, // How fast the meter falls
}

impl DbMeter {
    pub fn new() -> Self {
        Self {
            current_db: -100.0,
            release_speed: 0.15, // Adjust for that "smooth" feel
        }
    }

    pub fn process_buffer(&mut self, samples: &[f32]) {
        if samples.is_empty() { return; }

        // 1. Standard RMS calculation
        let sq_sum: f32 = samples.iter().map(|&s| s * s).sum();
        let rms = (sq_sum / samples.len() as f32).sqrt();

        // 2. Convert to dB
        let target_db = if rms > 0.00001 {
            20.0 * rms.log10()
        } else {
            -100.0
        };

        // 3. Apply Ballistics (Fast attack, slow release)
        if target_db > self.current_db {
            self.current_db = target_db; // Instant jump up
        } else {
            // Smoothly slide down
            self.current_db += (target_db - self.current_db) * self.release_speed;
        }
    }
}