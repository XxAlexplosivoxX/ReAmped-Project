//! Oscilloscope-style synchronized waveform extraction.
//!
//! The trigger pipeline resolves a stable trigger point from audio samples:
//!
//! 1. **YIN pitch detection** – estimates the fundamental period
//! 2. **FIR low-pass filter** – removes high-frequency noise with an
//!    adaptive cutoff that tracks the detected fundamental (~2.5×)
//! 3. **Sub-sample zero-crossing** – precise rising-edge (negative →
//!    positive) detection via cubic root finding, the only trigger used
//! 4. **Polarity lock** – the displayed wave is correlated against the
//!    previous frame and flipped when anti-correlated, keeping it stable on
//!    harmonically dense material
//!
//! A window of `4×` the estimated period is extracted around the trigger
//! point using cubic interpolation for sub-sample precision.  The extracted
//! frame is suitable for oscilloscope-style waveform display.

use std::sync::{Arc, Mutex};

/// Lowest frequency the YIN pitch detector can resolve.
const MIN_FREQ: f32 = 40.0;
/// Highest frequency the YIN pitch detector can resolve.
const MAX_FREQ: f32 = 2000.0;
/// YIN difference-function threshold below which a candidate period is accepted.
const YIN_THRESHOLD: f32 = 0.15;
/// Number of samples fed to the YIN pitch detector (2048 @ 44.1 kHz ≈ 46 ms).
const YIN_WINDOW: usize = 2048;
/// Number of taps for the FIR low-pass filter.
const FIR_NUM_TAPS: usize = 31;
/// Trigger low-pass cutoff tracks `this ×` the detected fundamental
/// (clamped to [`TRIGGER_CUTOFF_MIN`]–[`TRIGGER_CUTOFF_MAX`]) so the
/// oscilloscope locks cleanly even when the EQ boosts harmonics.
const TRIGGER_CUTOFF_MULT: f32 = 2.5;
/// Minimum trigger low-pass cutoff (Hz).
const TRIGGER_CUTOFF_MIN: f32 = 120.0;
/// Maximum trigger low-pass cutoff (Hz).
const TRIGGER_CUTOFF_MAX: f32 = 2000.0;
/// Re-design the FIR filter only when the cutoff moves more than this (Hz).
const FIR_REDESIGN_HYSTERESIS: f32 = 10.0;
/// Multiple of the estimated period used to set the waveform extraction window.
const PERIOD_MULTIPLIER: usize = 4;
/// Minimum period (in samples) to prevent division-by-zero and spurious triggers.
const MIN_PERIOD_SAMPLES: f32 = 22.0;
/// Minimum window duration (seconds) when extracting synchronised samples.
const MIN_WINDOW_SECS: f32 = 0.015;

// Thread-local cache of the trigger low-pass filter, re-designed whenever the
// adaptive cutoff moves more than [`FIR_REDESIGN_HYSTERESIS`] Hz.
thread_local! {
    static FIR_CACHE: std::cell::RefCell<(f32, Vec<f32>)> =
        std::cell::RefCell::new((0.0, Vec::new()));
}

/// Pre-allocated scratch buffers reused across frames to avoid heap churn.
struct ReusableBuffers {
    mono: Vec<f32>,
    d: Vec<f32>,
    cmnd: Vec<f32>,
    filtered: Vec<f32>,
}

thread_local! {
    static BUF: std::cell::RefCell<ReusableBuffers> = std::cell::RefCell::new(ReusableBuffers {
        mono: Vec::new(),
        d: Vec::new(),
        cmnd: Vec::new(),
        filtered: Vec::new(),
    });
}

/// A synchronised waveform frame produced by [`synchronized_waveform`].
///
/// Contains the extracted sample window, the estimated period (in samples),
/// the trigger position (sample index with sub-sample precision), the detected
/// pitch (if any), and the sample rate.
pub struct OscilloscopeFrame {
    pub samples: Vec<f32>,
    pub period: f32,
    pub trigger_pos: f32,
    pub pitch_hz: Option<f32>,
    pub sample_rate: f32,
}

/// Estimate the fundamental frequency of a mono signal using the YIN algorithm.
///
/// YIN computes a cumulative mean-normalised difference function (CMNDF) over
/// candidate lags.  The first lag whose CMNDF value falls below
/// [`YIN_THRESHOLD`] is selected, and a parabolic interpolation refines the
/// estimate for sub-sample precision.  The search range is
/// [`MIN_FREQ`]–[`MAX_FREQ`].
///
/// Uses pre-allocated scratch buffers `d` and `cmnd` to avoid heap churn.
fn yin_pitch_detection(
    samples: &[f32],
    sample_rate: f32,
    d: &mut Vec<f32>,
    cmnd: &mut Vec<f32>,
) -> Option<f32> {
    let min_period = (sample_rate / MAX_FREQ) as usize;
    let max_period = (sample_rate / MIN_FREQ) as usize;

    let len = samples.len();
    let max_lag = max_period.min(len / 2);
    let min_lag = min_period.min(max_lag);

    if max_lag < 2 || len < max_lag + 2 {
        return None;
    }

    d.resize(max_lag + 1, 0.0);
    for tau in 0..=max_lag {
        let mut sum = 0.0;
        let n = len - max_lag - 1;
        if n == 0 {
            continue;
        }
        for j in 0..n {
            let delta = samples[j] - samples[j + tau];
            sum += delta * delta;
        }
        d[tau] = sum;
    }

    let mut running_sum = 0.0;
    cmnd.resize(max_lag + 1, 0.0);
    cmnd[0] = 1.0;
    for tau in 1..=max_lag {
        running_sum += d[tau];
        cmnd[tau] = if running_sum == 0.0 {
            1.0
        } else {
            d[tau] * tau as f32 / running_sum
        };
    }

    let mut best_tau = min_lag;
    let mut best_value = cmnd[min_lag];
    let mut found_threshold = false;

    for tau in min_lag..=max_lag {
        let v = cmnd[tau];
        if v < best_value {
            best_value = v;
            best_tau = tau;
        }
        if v < YIN_THRESHOLD {
            best_tau = tau;
            best_value = v;
            found_threshold = true;
            break;
        }
    }

    if !found_threshold && best_value >= YIN_THRESHOLD * 2.0 {
        return None;
    }

    let tau_f = if best_tau > 0 && best_tau < max_lag {
        let a = cmnd[best_tau - 1];
        let b = cmnd[best_tau];
        let c = cmnd[best_tau + 1];
        let denom = a + c - 2.0 * b;
        if denom.abs() > 1e-12 {
            let shift = (a - c) / (2.0 * denom);
            (best_tau as f32 + shift.clamp(-1.0, 1.0)).max(1.0)
        } else {
            best_tau as f32
        }
    } else {
        best_tau as f32
    };

    if tau_f >= 1.0 {
        Some(sample_rate / tau_f)
    } else {
        None
    }
}

/// Design a low-pass FIR filter using the windowed-sinc method with a
/// Blackman window.
///
/// Normalised cut-off at `cutoff_hz / sample_rate`.  Coefficients are
/// normalised so the filter has unity gain at DC.
fn design_lowpass_fir(cutoff_hz: f32, sample_rate: f32, num_taps: usize) -> Vec<f32> {
    let mut coeffs = vec![0.0; num_taps];
    let fc = cutoff_hz / sample_rate;
    let half = num_taps as f32 / 2.0;

    for i in 0..num_taps {
        let n = i as f32 - half;
        if n.abs() < 1e-8 {
            coeffs[i] = 2.0 * fc;
        } else {
            coeffs[i] =
                (std::f32::consts::PI * 2.0 * fc * n).sin() / (std::f32::consts::PI * n);
        }
        let bm = 0.42
            - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (num_taps - 1) as f32).cos()
            + 0.08 * (4.0 * std::f32::consts::PI * i as f32 / (num_taps - 1) as f32).cos();
        coeffs[i] *= bm;
    }

    let sum: f32 = coeffs.iter().sum();
    if sum.abs() > 1e-12 {
        for c in &mut coeffs {
            *c /= sum;
        }
    }

    coeffs
}

/// Apply an FIR convolution (direct-form) to a sample buffer.
///
/// The output has the same length as the input; samples near the edges are
/// zero-padded implicitly (out-of-range indices are skipped).
/// Writes into `out` (resized/overwritten as needed).
fn apply_fir_filter(samples: &[f32], coeffs: &[f32], out: &mut Vec<f32>) {
    let len = samples.len();
    let num_taps = coeffs.len();
    let delay = num_taps / 2;
    out.clear();
    out.reserve(len);
    for i in 0..len {
        let mut sum = 0.0;
        for j in 0..num_taps {
            let idx = i as isize + j as isize - delay as isize;
            if idx >= 0 && idx < len as isize {
                sum += samples[idx as usize] * coeffs[j];
            }
        }
        out.push(sum);
    }
}

/// Low-pass `samples` into `out` with an adaptive cutoff, re-using a cached
/// FIR design until the cutoff drifts beyond [`FIR_REDESIGN_HYSTERESIS`] Hz.
fn apply_trigger_filter(samples: &[f32], cutoff_hz: f32, sample_rate: f32, out: &mut Vec<f32>) {
    FIR_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if (cache.0 - cutoff_hz).abs() > FIR_REDESIGN_HYSTERESIS {
            cache.0 = cutoff_hz;
            cache.1 = design_lowpass_fir(cutoff_hz, sample_rate, FIR_NUM_TAPS);
        }
        apply_fir_filter(samples, &cache.1, out);
    });
}

/// Cubic Hermite interpolation at a fractional sample index.
///
/// Uses four neighbours (`i-1`, `i`, `i+1`, `i+2`) and a Catmull-Rom
/// Hermite basis to produce a smooth curve through the sample points.
fn cubic_interpolate(samples: &[f32], idx: f32) -> f32 {
    let i = idx as isize;
    let t = idx - i as f32;
    let len = samples.len();

    let clamp = |p: isize| -> usize { p.max(0).min(len as isize - 1) as usize };

    let y0 = samples[clamp(i - 1)];
    let y1 = samples[clamp(i)];
    let y2 = samples[clamp(i + 1)];
    let y3 = samples[clamp(i + 2)];

    let c0 = y1;
    let c1 = 0.5 * (-y0 + y2);
    let c2 = 0.5 * (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3);
    let c3 = 0.5 * (-y0 + 3.0 * y1 - 3.0 * y2 + y3);

    ((c3 * t + c2) * t + c1) * t + c0
}

/// Find the root of a cubic Hermite polynomial between `y1` and `y2`
/// using Newton's method.
///
/// The polynomial is defined by the four control points `y0`–`y3`.
/// The initial guess is a linear interpolation at the zero-crossing
/// of the line through `y1` and `y2`.
fn find_cubic_root(y0: f32, y1: f32, y2: f32, y3: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (-y0 + y2);
    let c2 = 0.5 * (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3);
    let c3 = 0.5 * (-y0 + 3.0 * y1 - 3.0 * y2 + y3);

    let mut t = -y1 / (y2 - y1);
    t = t.clamp(0.0, 1.0);

    for _ in 0..8 {
        let ft = ((c3 * t + c2) * t + c1) * t + c0;
        if ft.abs() < 1e-8 {
            break;
        }
        let fpt = (3.0 * c3 * t + 2.0 * c2) * t + c1;
        if fpt.abs() < 1e-12 {
            break;
        }
        t -= ft / fpt;
        t = t.clamp(0.0, 1.0);
    }

    t
}

/// Locate the first rising-edge zero-crossing with sub-sample precision.
///
/// Searches for a transition from negative to non-negative.  When found,
/// [`find_cubic_root`] is used to compute the crossing position with
/// sub-sample accuracy.  Crossings inside the FIR filter transient
/// (first 15 samples) are skipped unless they are the only candidate.
fn sub_sample_zero_crossing(samples: &[f32], start: usize) -> Option<f32> {
    let len = samples.len();
    let mut first: Option<f32> = None;
    let mut i = start.max(1);

    while i < len - 2 {
        if samples[i - 1] < 0.0 && samples[i] >= 0.0 {
            let y0 = if i >= 2 { samples[i - 2] } else { samples[i - 1] };
            let y1 = samples[i - 1];
            let y2 = samples[i];
            let y3 = if i + 1 < len { samples[i + 1] } else { samples[i] };

            let t = find_cubic_root(y0, y1, y2, y3);
            let pos = (i - 1) as f32 + t;

            // If the very first crossing is outside the FIR transient → use it.
            if first.is_none() {
                first = Some(pos);
                if i >= 15 {
                    return first;
                }
            }
            // If we found a crossing inside the transient, keep going:
            // prefer the first crossing beyond the transient region.
            if i >= 15 {
                return Some(pos);
            }
        }
        i += 1;
    }

    first
}

/// Extract a window of `num_samples` starting at `trigger` using cubic interpolation.
///
/// Positions outside the buffer are padded with zero.
fn extract_window(samples: &[f32], trigger: f32, num_samples: usize) -> Vec<f32> {
    let mut result = Vec::with_capacity(num_samples);
    let len = samples.len();

    for i in 0..num_samples {
        let pos = trigger + i as f32;
        if pos < 0.0 || pos >= (len - 1) as f32 {
            result.push(0.0);
        } else {
            result.push(cubic_interpolate(samples, pos));
        }
    }

    result
}

/// Correct octave errors in period estimates by comparing with the previous frame.
///
/// If the new period is roughly double (`ratio ∈ (1.7, 2.35)`) or half
/// (`ratio ∈ (0.4, 0.6)`) the previous period, it is halved or doubled
/// respectively to resolve the octave ambiguity.
fn correct_octave(p: f32, prev: f32) -> f32 {
    if prev <= 0.0 {
        return p;
    }
    let ratio = p / prev;
    if ratio > 1.7 && ratio < 2.35 {
        p * 0.5
    } else if ratio > 0.4 && ratio < 0.6 {
        p * 2.0
    } else {
        p
    }
}

/// Compute the RMS amplitude of a sample buffer.
fn compute_rms(samples: &[f32]) -> f32 {
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Snap a trigger position to the nearest rising-edge zero-crossing (negative →
/// non-negative) within ±half a period.
///
/// Uses sub-sample precision via [`find_cubic_root`].  If no rising crossing is
/// found in the search window, the original trigger is returned unchanged.
fn snap_to_rising_zero(samples: &[f32], trigger: f32, period: f32) -> f32 {
    let len = samples.len();
    if len < 4 || period < 2.0 {
        return trigger;
    }

    let search = (period * 0.5).ceil() as isize;
    let center = trigger.round() as isize;
    let start = (center - search).max(1) as usize;
    let end = (center + search).min(len as isize - 2) as usize;

    let mut best_dist = f32::MAX;
    let mut best_pos = trigger;

    let mut i = start.max(1);
    while i <= end && i < len - 1 {
        if samples[i - 1] < 0.0 && samples[i] >= 0.0 {
            let y0 = if i >= 2 { samples[i - 2] } else { samples[i - 1] };
            let y1 = samples[i - 1];
            let y2 = samples[i];
            let y3 = if i + 1 < len { samples[i + 1] } else { samples[i] };
            let t = find_cubic_root(y0, y1, y2, y3);
            let pos = (i - 1) as f32 + t;
            let dist = (pos - trigger).abs();
            if dist < best_dist {
                best_dist = dist;
                best_pos = pos;
            }
        }
        i += 1;
    }

    best_pos
}

/// Extract a synchronised (trigger-aligned) waveform window from a stereo
/// ring buffer.
///
/// The input is stereo interleaved (`L,R,L,R,…`).  It is down-mixed to
/// mono, then the trigger pipeline runs:
///
/// 1. **FIR low-pass filter** (adaptive cutoff from the previous frame's
///    period) — also the input for YIN so high-frequency content cannot fool
///    pitch detection
/// 2. **YIN pitch detection** (windowed to [`YIN_WINDOW`] samples) → estimated
///    period (smoothed per-frame with octave correction and exponential
///    averaging)
/// 3. **Sub-sample zero-crossing** — rising edge (negative → positive) only,
///    refined by [`snap_to_rising_zero`]
/// 4. **Polarity lock** against `last_window` so the wave stays stable instead
///    of "turning over" on harmonically dense material
///
/// The extracted window is `4 × period` samples long, taken from the raw mono
/// signal with cubic interpolation.  `last_window` is overwritten with the
/// frame actually displayed, for use as the reference next frame.
pub fn synchronized_waveform(
    samples: Arc<Mutex<Vec<f32>>>,
    count: usize,
    sample_rate: f32,
    last_period: &mut f32,
    last_window: &mut Vec<f32>,
) -> OscilloscopeFrame {
    let buf = samples.lock().unwrap();

    let stereo_len = buf.len();
    let take_frames = count.min(stereo_len / 2);

    BUF.with(|cell| {
        let mut guard = cell.borrow_mut();
        let rb = &mut *guard;

        rb.mono.clear();
        rb.mono.reserve(take_frames);

        let mut idx = stereo_len;
        for _ in 0..take_frames {
            if idx < 2 {
                break;
            }
            idx -= 2;
            let l = buf[idx];
            let r = buf[idx + 1];
            rb.mono.push((l + r) * 0.5);
        }
        drop(buf);

        if rb.mono.len() < 8 {
            return OscilloscopeFrame {
                samples: std::mem::take(&mut rb.mono),
                period: *last_period,
                trigger_pos: 0.0,
                pitch_hz: None,
                sample_rate,
            };
        }

        if compute_rms(&rb.mono) < 0.001 {
            return OscilloscopeFrame {
                samples: std::mem::take(&mut rb.mono),
                period: *last_period,
                trigger_pos: 0.0,
                pitch_hz: None,
                sample_rate,
            };
        }

        let yin_len = rb.mono.len().min(YIN_WINDOW);

        // Pitch detection runs on a low-passed copy so heavy 1kHz–20kHz content
        // cannot fool YIN into locking onto high harmonics. The cutoff is derived
        // from the previous frame's period (800 Hz until one is known).
        let yin_cutoff = if *last_period > 0.0 {
            (sample_rate / *last_period * TRIGGER_CUTOFF_MULT)
                .clamp(TRIGGER_CUTOFF_MIN, TRIGGER_CUTOFF_MAX)
        } else {
            800.0
        };
        apply_trigger_filter(&rb.mono, yin_cutoff, sample_rate, &mut rb.filtered);

        let pitch = yin_pitch_detection(&rb.filtered[..yin_len], sample_rate, &mut rb.d, &mut rb.cmnd);
        let new_period = pitch.map(|f| (sample_rate / f).max(MIN_PERIOD_SAMPLES));

        if let Some(p) = new_period {
            if *last_period <= 0.0 {
                *last_period = p;
            } else {
                let corrected = correct_octave(p, *last_period);
                *last_period = *last_period * 0.85 + corrected * 0.15;
            }
        }
        *last_period = last_period.max(MIN_PERIOD_SAMPLES);

        let period = *last_period;

        // The trigger filter tracks the detected fundamental so the oscilloscope
        // locks at a consistent phase even when the EQ boosts harmonics or the
        // fundamental sits above the old fixed 800 Hz cutoff.
        let fundamental_hz = sample_rate / period.max(1.0);
        let trigger_cutoff =
            (fundamental_hz * TRIGGER_CUTOFF_MULT).clamp(TRIGGER_CUTOFF_MIN, TRIGGER_CUTOFF_MAX);

        let min_window = (sample_rate * MIN_WINDOW_SECS) as usize;
        let window_size = (PERIOD_MULTIPLIER as f32 * period).ceil() as usize;
        let window_size = window_size.max(min_window).min(rb.mono.len() / 2);

        apply_trigger_filter(&rb.mono, trigger_cutoff, sample_rate, &mut rb.filtered);

        let trigger = sub_sample_zero_crossing(&rb.filtered, 0).unwrap_or(0.0);

        let trigger = snap_to_rising_zero(&rb.filtered, trigger, period);

        let remaining = rb.mono.len() as f32 - trigger;
        let actual_window = window_size.min(remaining as usize).max(4);
        let mut sync_samples = extract_window(&rb.mono, trigger, actual_window);

        // Polarity lock: if the new window anti-correlates with the previous
        // displayed one, the trigger landed a half period away — flip it back
        // so the wave stays stable instead of "turning over" on harmonically
        // dense material.
        if !last_window.is_empty() {
            let n = sync_samples.len().min(last_window.len());
            if n >= 8 {
                let mut dot = 0.0;
                for k in 0..n {
                    dot += sync_samples[k] * last_window[k];
                }
                let norm = last_window.iter().map(|v| v * v).sum::<f32>().max(1e-9);
                if dot / norm < 0.0 {
                    for s in &mut sync_samples {
                        *s = -*s;
                    }
                }
            }
        }
        last_window.clear();
        last_window.extend_from_slice(&sync_samples);

        OscilloscopeFrame {
            samples: sync_samples,
            period,
            trigger_pos: trigger,
            pitch_hz: pitch,
            sample_rate,
        }
    })
}
