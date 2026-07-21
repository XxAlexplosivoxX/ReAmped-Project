//! Three-band equalizer with low-shelf, peaking, and high-shelf filters.
//!
//! Each band is implemented as a second-order IIR biquad in Direct Form I
//! (transposed). The [`TripleBandEq`] cascades three biquads in series:
//!
//! 1. Low shelf at 150 Hz
//! 2. Peaking EQ at 1 kHz
//! 3. High shelf at 6 kHz
//!
//! Coefficients are computed using the RBJ audio EQ cookbook formulas.

use std::f32::consts::PI;

/// Second-order IIR biquad filter using the Direct Form I (transposed) structure.
///
/// Maintains two delay-line states (`w1`, `w2`) and processes a single sample
/// via the difference equation:
///
/// ```text
/// y[n] = b0·x[n] + w1
/// w1   = b1·x[n] − a1·y[n] + w2
/// w2   = b2·x[n] − a2·y[n]
/// ```
///
/// Coefficients are set via [`BiquadCoeffs`] and updated with [`Biquad::update_coeffs`].
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
/// Coefficients for a second-order IIR biquad filter in Direct Form I.
///
/// `a1` and `a2` are the recursive (feedback) coefficients; `b0`, `b1`, `b2`
/// are the feedforward coefficients.  All values have been normalised so that
/// the implicit `a0` denominator is 1.0.
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Biquad {
    /// Replace the filter coefficients with a new set of [`BiquadCoeffs`].
    pub fn update_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.b0 = coeffs.b0;
        self.b1 = coeffs.b1;
        self.b2 = coeffs.b2;
        self.a1 = coeffs.a1;
        self.a2 = coeffs.a2;
    }

    /// Create a new biquad initialised as a pass-through (gain = 1.0).
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
    /// Process a single audio sample through the filter and return the output.
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.w1;
        self.w1 = self.b1 * x - self.a1 * y + self.w2;
        self.w2 = self.b2 * x - self.a2 * y;
        y
    }
}

impl BiquadCoeffs {
    /// Compute coefficients for a low-shelf filter using the RBJ cookbook.
    ///
    /// # Arguments
    ///
    /// * `gain` – Linear gain (0.0–2.0, 1.0 ≡ 0 dB)
    /// * `freq` – Shelf centre frequency in Hz
    /// * `sample_rate` – Sample rate in Hz (e.g. 44100.0)
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

    /// Compute coefficients for a high-shelf filter using the RBJ cookbook.
    ///
    /// # Arguments
    ///
    /// * `gain` – Linear gain (0.0–2.0, 1.0 ≡ 0 dB)
    /// * `freq` – Shelf centre frequency in Hz
    /// * `sample_rate` – Sample rate in Hz (e.g. 44100.0)
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

    /// Compute coefficients for a peaking (bell) EQ filter using the RBJ cookbook.
    ///
    /// Q is fixed at 0.707 (Butterworth response).
    /// The gain is clamped so the underlying dB value stays above −24 dB.
    ///
    /// # Arguments
    ///
    /// * `gain` – Linear gain (0.0–2.0, 1.0 ≡ 0 dB)
    /// * `freq` – Centre frequency in Hz
    /// * `sample_rate` – Sample rate in Hz (e.g. 44100.0)
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

/// Cascaded three-band equalizer comprising low-shelf, peaking, and high-shelf filters.
///
/// The signal path is:
/// 1. Low-shelf biquad at 150 Hz
/// 2. Peaking EQ biquad at 1 kHz
/// 3. High-shelf biquad at 6 kHz
pub struct TripleBandEq {
    pub low: Biquad,
    pub mid: Biquad,
    pub high: Biquad,
}

impl TripleBandEq {
    /// Create a new [`TripleBandEq`] with all three bands initialised as pass-through.
    pub fn new() -> Self {
        Self {
            low: Biquad::new(),
            mid: Biquad::new(),
            high: Biquad::new(),
        }
    }

    /// Update all three bands with new gain values.
    ///
    /// # Arguments
    ///
    /// * `low_g`  – Low-shelf gain (0.0–2.0, 1.0 ≡ 0 dB)
    /// * `mid_g`  – Peaking EQ gain (0.0–2.0, 1.0 ≡ 0 dB)
    /// * `high_g` – High-shelf gain (0.0–2.0, 1.0 ≡ 0 dB)
    /// * `sample_rate` – Sample rate in Hz
    pub fn update_all(&mut self, low_g: f32, mid_g: f32, high_g: f32, sample_rate: f32) {
        self.low
            .update_coeffs(BiquadCoeffs::low_shelf(low_g, 150.0, sample_rate));
        self.mid
            .update_coeffs(BiquadCoeffs::peaking_eq(mid_g, 1000.0, sample_rate));
        self.high
            .update_coeffs(BiquadCoeffs::high_shelf(high_g, 6000.0, sample_rate));
    }

    /// Process a single audio sample through the cascaded low → mid → high chain.
    pub fn process(&mut self, sample: f32) -> f32 {
        let s1 = self.low.process(sample);
        let s2 = self.mid.process(s1);
        let s3 = self.high.process(s2);
        s3
    }
}
