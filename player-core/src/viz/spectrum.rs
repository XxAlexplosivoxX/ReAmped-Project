//! FFT-based spectrum analysis with logarithmic frequency band mapping.
//!
//! The analysis pipeline:
//!
//! 1. Mix stereo input to mono by averaging channels
//! 2. Apply a Hann window to reduce spectral leakage
//! 3. Compute forward real FFT via [rustfft]
//! 4. Extract magnitude spectrum (log-scaled with `ln(mag + 1e-6)`)
//! 5. Optionally apply spatial smoothing, remap to log-spaced bands, or
//!    slice to a specific frequency range

use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::{Arc, Mutex};

/// Compute the log-magnitude FFT spectrum of the most recent audio buffer.
///
/// # Arguments
///
/// * `samples` – Shared ring buffer of interleaved stereo samples (`L,R,L,R,…`)
/// * `size`    – Desired FFT size (must be a power of two)
///
/// # Returns
///
/// A vector of `size / 2` log-magnitude bins covering 0 Hz through the
/// Nyquist frequency.
pub fn spectrum(samples: Arc<Mutex<Vec<f32>>>, size: usize) -> Vec<f32> {
    let buf = samples.lock().unwrap();
    let frame_count = buf.len() / 2;

    if frame_count < 2 {
        return vec![0.0; size / 2];
    }

    let take = size.min(frame_count);
    let skip = frame_count - take;

    let mut mono = Vec::with_capacity(size);
    for i in 0..take {
        let idx = (skip + i) * 2;
        let l = buf[idx];
        let r = buf[idx + 1];
        mono.push((l + r) * 0.5);
    }
    mono.resize(size, 0.0);
    drop(buf);

    let mut input: Vec<Complex<f32>> = mono
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos());
            Complex {
                re: s * w,
                im: 0.0,
            }
        })
        .collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(size);
    fft.process(&mut input);

    input[..size / 2]
        .iter()
        .map(|c| {
            let mag = c.norm();
            (mag + 1e-6).ln()
        })
        .collect()
}

/// Apply a three-point spatial smoothing kernel to the spectrum.
///
/// Each bin (except the first and last third of the array) is replaced by
/// a weighted average of its neighbours:
///
/// ```text
/// out[i] = 0.25 × in[i-1] + 0.5 × in[i] + 0.25 × in[i+1]
/// ```
///
/// The first and last third of the array are left untouched.
pub fn smooth_spatial(input: &[f32]) -> Vec<f32> {
    let mut out = input.to_vec();

    for i in 1..input.len() - ((input.len()/3) * 2){
        out[i] = input[i - 1] * 0.25
               + input[i]     * 0.5
               + input[i + 1] * 0.25;
    }

    out
}


/// Remap a linear FFT spectrum into logarithmically-spaced frequency bands.
///
/// Each band covers an equal interval in log-frequency from `f_min` to `f_max`.
/// The magnitude for a band is the average of all FFT bins that fall within
/// its frequency range.
///
/// # Arguments
///
/// * `spectrum`   – Log-magnitude spectrum from [`spectrum`]
/// * `bands`      – Number of output bands
/// * `sample_rate` – Sample rate in Hz
/// * `fft_size`   – FFT size used to produce the spectrum
/// * `f_min`      – Lowest frequency of interest (Hz)
/// * `f_max`      – Highest frequency of interest (Hz)
pub fn log_frequency_bands(
    spectrum: &[f32],
    bands: usize,
    sample_rate: f32,
    fft_size: usize,
    f_min: f32,
    f_max: f32,
) -> Vec<f32> {
    let mut out = vec![0.0; bands];

    let min_log = f_min.ln();
    let max_log = f_max.ln();

    for i in 0..bands {
        let t0 = i as f32 / bands as f32;
        let t1 = (i + 1) as f32 / bands as f32;

        let f0 = (min_log + t0 * (max_log - min_log)).exp();
        let f1 = (min_log + t1 * (max_log - min_log)).exp();

        let bin0 = ((f0 / sample_rate) * fft_size as f32) as usize;
        let bin1 = ((f1 / sample_rate) * fft_size as f32) as usize;

        let slice = &spectrum[bin0.min(spectrum.len())..bin1.min(spectrum.len()).max(bin0 + 1)];

        if !slice.is_empty() {
            out[i] = slice.iter().sum::<f32>() / slice.len() as f32;
        }
    }

    out
}


/// Extract a contiguous slice of the spectrum between `f_min` and `f_max`.
///
/// Convenience wrapper around [`spectrum`] that returns only the bins
/// whose centre frequencies fall within the requested range.
pub fn spectrum_range(
    samples: Arc<Mutex<Vec<f32>>>,
    fft_size: usize,
    sample_rate: f32,
    f_min: f32,
    f_max: f32,
) -> Vec<f32> {
    let raw = spectrum(samples, fft_size);

    let bin_min = ((f_min / sample_rate) * fft_size as f32) as usize;
    let bin_max = ((f_max / sample_rate) * fft_size as f32) as usize;

    raw[bin_min..bin_max.min(raw.len())].to_vec()
}

/// Evenly remap a spectrum slice into a fixed number of output bars.
///
/// Each output bar is the average of a contiguous, equal-width segment of
/// the input data.  This is useful for driving a bar-graph visualiser with
/// a consistent number of bars regardless of FFT size.
pub fn remap_to_bars(data: &[f32], bars: usize) -> Vec<f32> {
    let len = data.len();
    let mut out = vec![0.0; bars];

    for i in 0..bars {
        let t0 = i as f32 / bars as f32;
        let t1 = (i + 1) as f32 / bars as f32;

        let i0 = (t0 * len as f32) as usize;
        let i1 = (t1 * len as f32) as usize;

        let slice = &data[i0..i1.max(i0 + 1)];
        out[i] = slice.iter().sum::<f32>() / slice.len() as f32;
    }

    out
}
