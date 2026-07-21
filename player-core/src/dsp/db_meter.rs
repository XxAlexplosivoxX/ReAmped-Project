//! RMS-based decibel level meter with ballistic response.
//!
//! The [`DbMeter`] computes the RMS power of an audio buffer, converts it
//! to dBFS, and applies a fast-attack / slow-release envelope for smooth
//! visual metering.  Attack is instantaneous (sample-accurate jump to new
//! peak), while release decays exponentially toward lower levels.

/// RMS level meter with fast-attack, slow-release ballistics.
///
/// Instantaneously jumps to new peak levels (attack) and smoothly decays
/// toward lower levels (release), mimicking analog VU meter behaviour.
pub struct DbMeter {
    pub current_db: f32,
    pub release_speed: f32,
}

impl DbMeter {
    /// Create a new [`DbMeter`] with the meter floored at −100 dBFS.
    pub fn new() -> Self {
        Self {
            current_db: -100.0,
            release_speed: 0.15,
        }
    }

    /// Process a buffer of samples and update the metered level.
    ///
    /// 1. Computes the RMS of all samples.
    /// 2. Converts to dBFS (floor at −100 dB).
    /// 3. Applies fast-attach / slow-release ballistics — the meter jumps
    ///    instantly to a new peak and decays exponentially toward silence.
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