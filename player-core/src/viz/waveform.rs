use std::sync::{Arc, Mutex};

const MIN_FREQ: f32 = 40.0;
const MAX_FREQ: f32 = 2000.0;
const YIN_THRESHOLD: f32 = 0.15;
const FIR_NUM_TAPS: usize = 31;
const FIR_CUTOFF: f32 = 800.0;
const PERIOD_MULTIPLIER: usize = 4;
const MIN_PERIOD_SAMPLES: f32 = 22.0;
const MIN_WINDOW_SECS: f32 = 0.015;

pub struct OscilloscopeFrame {
    pub samples: Vec<f32>,
    pub period: f32,
    pub trigger_pos: f32,
    pub pitch_hz: Option<f32>,
    pub sample_rate: f32,
}

fn yin_pitch_detection(samples: &[f32], sample_rate: f32) -> Option<f32> {
    let min_period = (sample_rate / MAX_FREQ) as usize;
    let max_period = (sample_rate / MIN_FREQ) as usize;

    let len = samples.len();
    let max_lag = max_period.min(len / 2);
    let min_lag = min_period.min(max_lag);

    if max_lag < 2 || len < max_lag + 2 {
        return None;
    }

    let mut diff = vec![0.0; max_lag + 1];
    for tau in 0..=max_lag {
        let mut sum = 0.0;
        let n = len - max_lag - 1;
        for j in 0..n {
            let d = samples[j] - samples[j + tau];
            sum += d * d;
        }
        diff[tau] = sum;
    }

    let mut running_sum = 0.0;
    let mut cmnd = vec![0.0; max_lag + 1];
    cmnd[0] = 1.0;
    for tau in 1..=max_lag {
        running_sum += diff[tau];
        cmnd[tau] = if running_sum == 0.0 {
            1.0
        } else {
            diff[tau] * tau as f32 / running_sum
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

fn apply_fir_filter(samples: &[f32], coeffs: &[f32]) -> Vec<f32> {
    let len = samples.len();
    let num_taps = coeffs.len();
    let delay = num_taps / 2;
    let mut out = vec![0.0; len];

    for i in 0..len {
        let mut sum = 0.0;
        for j in 0..num_taps {
            let idx = i as isize + j as isize - delay as isize;
            if idx >= 0 && idx < len as isize {
                sum += samples[idx as usize] * coeffs[j];
            }
        }
        out[i] = sum;
    }

    out
}

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

fn sub_sample_zero_crossing(samples: &[f32], start: usize) -> Option<f32> {
    let len = samples.len();
    let mut i = start.max(1);

    while i < len - 2 {
        if samples[i - 1] < 0.0 && samples[i] >= 0.0 {
            let y0 = if i >= 2 { samples[i - 2] } else { samples[i - 1] };
            let y1 = samples[i - 1];
            let y2 = samples[i];
            let y3 = if i + 1 < len { samples[i + 1] } else { samples[i] };

            let t = find_cubic_root(y0, y1, y2, y3);
            return Some((i - 1) as f32 + t);
        }
        i += 1;
    }

    None
}

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

fn correlation_trigger(samples: &[f32], period: f32) -> Option<f32> {
    let p = period.round() as usize;
    let len = samples.len();
    if p < 2 || p * 2 >= len {
        return None;
    }

    let search_end = (len / 3).min(len - p * 2).min(p * 4);
    let window = p.min(64);
    if search_end < 1 || window < 1 {
        return None;
    }

    let mut best = 0.0f32;
    let mut best_corr = f32::MIN;

    for t in 0..search_end {
        let mut corr = 0.0;
        for j in 0..window {
            let a = samples[t + j];
            let b = samples[t + j + p];
            corr += a * b;
        }
        if corr > best_corr {
            best_corr = corr;
            best = t as f32;
        }
    }

    if best_corr > 1e-6 {
        Some(best)
    } else {
        None
    }
}

fn find_peak_position(samples: &[f32], start: usize) -> Option<f32> {
    let len = samples.len();
    if start >= len {
        return None;
    }
    let mut max_idx = start;
    let mut max_val = samples[start].abs();
    for i in (start + 1)..len {
        let val = samples[i].abs();
        if val > max_val {
            max_val = val;
            max_idx = i;
        }
    }
    if max_val > 0.001 {
        Some(max_idx as f32)
    } else {
        None
    }
}

fn compute_rms(samples: &[f32]) -> f32 {
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

pub fn synchronized_waveform(
    samples: Arc<Mutex<Vec<f32>>>,
    count: usize,
    sample_rate: f32,
    last_period: &mut f32,
) -> OscilloscopeFrame {
    let buf = samples.lock().unwrap();

    let stereo_len = buf.len();
    let take_frames = count.min(stereo_len / 2);
    let mut stereo: Vec<f32> = buf
        .iter()
        .rev()
        .take(take_frames * 2)
        .cloned()
        .collect();
    stereo.reverse();
    drop(buf);

    let mono: Vec<f32> = stereo
        .chunks(2)
        .filter(|ch| ch.len() == 2)
        .map(|ch| (ch[0] + ch[1]) * 0.5)
        .collect();

    if mono.len() < 8 {
        return OscilloscopeFrame {
            samples: mono,
            period: *last_period,
            trigger_pos: 0.0,
            pitch_hz: None,
            sample_rate,
        };
    }

    if compute_rms(&mono) < 0.001 {
        return OscilloscopeFrame {
            samples: mono,
            period: *last_period,
            trigger_pos: 0.0,
            pitch_hz: None,
            sample_rate,
        };
    }

    let pitch = yin_pitch_detection(&mono, sample_rate);
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

    let min_window = (sample_rate * MIN_WINDOW_SECS) as usize;
    let window_size = (PERIOD_MULTIPLIER as f32 * period).ceil() as usize;
    let window_size = window_size.max(min_window).min(mono.len() / 2);

    let fir_coeffs = design_lowpass_fir(FIR_CUTOFF, sample_rate, FIR_NUM_TAPS);
    let filtered = apply_fir_filter(&mono, &fir_coeffs);

    let trigger = sub_sample_zero_crossing(&filtered, FIR_NUM_TAPS / 2);

    let trigger = trigger.or_else(|| {
        if pitch.is_some() {
            correlation_trigger(&mono, period)
        } else {
            None
        }
    });

    let trigger = trigger.or_else(|| find_peak_position(&filtered, FIR_NUM_TAPS / 2));

    let trigger = trigger.unwrap_or(0.0);

    let remaining = mono.len() as f32 - trigger;
    let actual_window = window_size.min(remaining as usize).max(4);
    let sync_samples = extract_window(&mono, trigger, actual_window);

    OscilloscopeFrame {
        samples: sync_samples,
        period,
        trigger_pos: trigger,
        pitch_hz: pitch,
        sample_rate,
    }
}
