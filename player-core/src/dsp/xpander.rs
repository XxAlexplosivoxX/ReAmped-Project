pub struct Expander {
    pub width: f32, // width: 0.0 to 2.0 (1.0 is neutral)
}

impl Expander {
    pub fn new() -> Self {
        Self { width: 1.0 }
    }

    pub fn process_stereo_width(&self, left: f32, right: f32) -> (f32, f32) {
        // 1. Convert to Mid-Side
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;

        // 2. Scale the Side channel
        let new_side = side * self.width;

        let mut new_left = mid + new_side;
        let mut new_right = mid - new_side;

        // Simple automatic gain compensation
        let gain_reducer = 1.0 / (1.0 + (self.width - 1.0).max(0.0) * 0.5);
        new_left *= gain_reducer;
        new_right *= gain_reducer;

        (new_left, new_right)
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }
}
