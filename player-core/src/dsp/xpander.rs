//! Mid/side stereo width processor.
//!
//! Converts a stereo pair to mid/side representation, scales the side
//! channel by a user-controlled [`width`](Expander::width) factor, and
//! applies automatic gain compensation to prevent clipping when widening.

/// Stereo width expander using mid/side (M/S) processing.
///
/// The `width` parameter controls how much the side (difference) channel
/// is scaled relative to the mid (sum) channel:
///
/// | `width` | Effect                          |
/// |---------|---------------------------------|
/// | `0.0`   | Fully mono (side = 0)           |
/// | `1.0`   | Neutral (original stereo image) |
/// | `2.0`   | Double the original stereo width |
///
/// Automatic gain compensation reduces the overall level when `width > 1.0`
/// to avoid clipping.
pub struct Expander {
    pub width: f32,
}

impl Expander {
    /// Create a new [`Expander`] with neutral width (1.0).
    pub fn new() -> Self {
        Self { width: 1.0 }
    }

    /// Process a stereo pair through the mid/side width algorithm.
    ///
    /// The mid (sum) channel is preserved; the side (difference) channel is
    /// scaled by [`width`](Expander::width).  Automatic gain compensation
    /// prevents clipping when `width > 1.0`.
    pub fn process_stereo_width(&self, left: f32, right: f32) -> (f32, f32) {
        // 1. Convert to Mid-Side
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;

        // 2. Scale the Side channel
        let new_side = side * self.width;

        let mut new_left = mid + new_side;
        let mut new_right = mid - new_side;

        // Simple automatic gain compensation
        let gain_reducer = 1.0 / (1.0 + (self.width - 1.0).max(0.0) * 0.1);
        new_left *= gain_reducer;
        new_right *= gain_reducer;

        (new_left, new_right)
    }

    /// Set the stereo width factor.
    ///
    /// 0.0 = mono, 1.0 = original stereo, >1.0 = widened.
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }
}
