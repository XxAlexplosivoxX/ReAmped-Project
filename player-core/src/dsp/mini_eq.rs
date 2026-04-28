use std::f32::consts::PI;

pub struct Biquad {
    // Coefficients
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    // Delay lines (memory)
    w1: f32,
    w2: f32,
}
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Biquad {
    pub fn update_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.b0 = coeffs.b0;
        self.b1 = coeffs.b1;
        self.b2 = coeffs.b2;
        self.a1 = coeffs.a1;
        self.a2 = coeffs.a2;
    }

    pub fn new() -> Self {
        Self {
            a1: 0.0,
            a2: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            w1: 0.0,
            w2: 0.0,
        }
    }
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.w1;
        self.w1 = self.b1 * x - self.a1 * y + self.w2;
        self.w2 = self.b2 * x - self.a2 * y;
        y
    }
}

impl BiquadCoeffs {
    /// gain: 0.0 to 2.0 (1.0 is neutral/0dB)
    /// freq: center frequency in Hz
    /// sample_rate: usually 44100.0 or 48000.0
    pub fn low_shelf(gain: f32, freq: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(20.0 * gain.log10().max(-2.0) / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / 2.0 * (1.0 / 0.707); // Q = 0.707
        let cos_w0 = w0.cos();
        let sqrt_a_2_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + sqrt_a_2_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - sqrt_a_2_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + sqrt_a_2_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - sqrt_a_2_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    pub fn high_shelf(gain: f32, freq: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(20.0 * gain.log10().max(-2.0) / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / 2.0 * (1.0 / 0.707);
        let cos_w0 = w0.cos();
        let sqrt_a_2_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + sqrt_a_2_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - sqrt_a_2_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + sqrt_a_2_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - sqrt_a_2_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    pub fn peaking_eq(gain: f32, freq: f32, sample_rate: f32) -> Self {
        // 1. Convert 0.0..2.0 range to Decibels (-24dB to +6dB roughly)
        // 1.0 becomes 0dB.
        let gain_db = 20.0 * gain.log10().max(-2.0); // Clamp to avoid log(0)

        let a = 10.0_f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * freq / sample_rate;
        let sn = omega.sin();
        let cs = omega.cos();
        let q = 0.707; // Standard "Musical" width
        let alpha = sn / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cs;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cs;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

pub struct TripleBandEq {
    pub low: Biquad,
    pub mid: Biquad,
    pub high: Biquad,
}

impl TripleBandEq {
    pub fn new() -> Self {
        Self {
            low: Biquad::new(),
            mid: Biquad::new(),
            high: Biquad::new(),
        }
    }

    pub fn update_all(&mut self, low_g: f32, mid_g: f32, high_g: f32, sample_rate: f32) {
        self.low
            .update_coeffs(BiquadCoeffs::low_shelf(low_g, 150.0, sample_rate));
        self.mid
            .update_coeffs(BiquadCoeffs::peaking_eq(mid_g, 1000.0, sample_rate));
        self.high
            .update_coeffs(BiquadCoeffs::high_shelf(high_g, 6000.0, sample_rate));
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        // 1. Filter the raw sample through the Low band
        let s1 = self.low.process(sample);

        // 2. Filter that result through the Mid band
        let s2 = self.mid.process(s1);

        // 3. Filter that result through the High band
        let s3 = self.high.process(s2);

        // Return the final "triple-filtered" sample
        s3
    }
}
