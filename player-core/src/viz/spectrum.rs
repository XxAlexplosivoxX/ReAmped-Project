use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::{Arc, Mutex};

pub fn spectrum(samples: Arc<Mutex<Vec<f32>>>, size: usize) -> Vec<f32> {
    let buf = samples.lock().unwrap();

    if buf.len() < size {
        return vec![0.0; size / 2];
    }

    let mut input: Vec<Complex<f32>> = buf
        .iter()
        .rev()
        .take(size)
        .rev()
        .enumerate()
        .map(|(i, &s)| {
            // ventana Hann
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
            // compresión log
            (mag + 1e-6).ln()
        })
        .collect()
}

pub fn smooth_spatial(input: &[f32]) -> Vec<f32> {
    let mut out = input.to_vec();

    for i in 1..input.len() - ((input.len()/3) * 2){
        out[i] = input[i - 1] * 0.25
               + input[i]     * 0.5
               + input[i + 1] * 0.25;
    }

    out
}


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
