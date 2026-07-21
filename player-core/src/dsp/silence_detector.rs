use std::{
    fs::File,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use symphonia::{
    core::{
        audio::SampleBuffer,
        codecs::DecoderOptions,
        formats::{FormatOptions, SeekMode, SeekTo},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_probe,
};

/// Default silence threshold in dBFS (user requirement).
const DEFAULT_THRESHOLD_DB: f32 = -60.0;

/// Maximum seconds to scan from the start / end of each track.
const MAX_SCAN_SECS: f32 = 15.0;

/// Minimum consecutive silence to consider valid (avoids false positives
/// on brief pauses in audio).
const MIN_SILENCE_SECS: f32 = 0.05;

/// Number of samples per RMS measurement window.
const WINDOW_SIZE: usize = 4096;

fn rms_db(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    if sum <= 0.0 {
        return -100.0;
    }
    let rms = (sum / samples.len() as f32).sqrt();
    20.0 * rms.log10()
}

/// Decode up to `duration_seconds` starting at `seek_seconds`.
/// Returns `(samples_interleaved, sample_rate, channels)`.
fn decode_region(
    path: &Path,
    seek_seconds: f64,
    duration_seconds: f32,
    alive: &AtomicBool,
) -> Result<(Vec<f32>, usize, usize), String> {
    let file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("probe: {e:?}"))?;

    let mut format = probed.format;
    let track = format.default_track().ok_or("no default track")?;
    let channels = track.codec_params.channels.ok_or("no channels")?.count();
    let input_sr = track.codec_params.sample_rate.ok_or("no sample rate")? as usize;

    let mut params = track.codec_params.clone();
    params.sample_rate = Some(input_sr as u32);
    let mut decoder =
        symphonia::default::get_codecs().make(&params, &DecoderOptions::default())
            .map_err(|e| format!("decoder: {e:?}"))?;

    if seek_seconds > 0.0 {
        let _ = format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: seek_seconds.into(),
                track_id: Some(track.id),
            },
        );
    }

    let target_samples = (duration_seconds * input_sr as f32) as usize * channels;
    let mut output = Vec::with_capacity(target_samples.min(65536));
    let start = Instant::now();

    while alive.load(Ordering::SeqCst) {
        // Safety: don't hang on corrupt files
        if start.elapsed().as_secs_f32() > 30.0 {
            break;
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buf.copy_interleaved_ref(decoded);

        for s in buf.samples() {
            output.push(*s);
            if output.len() >= target_samples {
                return Ok((output, input_sr, channels));
            }
        }
    }
    Ok((output, input_sr, channels))
}

/// Scan the beginning of `samples` and return how many seconds of silence
/// (below `threshold_db`) are at the start.
fn scan_leading_silence(samples: &[f32], sample_rate: usize, threshold_db: f32, channels: usize) -> f32 {
    let mut silent_secs = 0.0f32;
    let window = WINDOW_SIZE.min(sample_rate);

    for chunk in samples.chunks(window * channels) {
        let mono: Vec<f32> = if channels >= 2 {
            chunk.chunks(2).map(|c| (c[0] + c[1]) * 0.5).collect()
        } else {
            chunk.to_vec()
        };
        if mono.len() < window / 4 {
            break;
        }
        let db = rms_db(&mono);
        if db > threshold_db {
            break;
        }
        silent_secs += mono.len() as f32 / sample_rate as f32;
    }
    silent_secs
}

/// Scan the end of `samples` and return how many seconds of silence
/// are at the tail (working backwards).
fn scan_trailing_silence(samples: &[f32], sample_rate: usize, threshold_db: f32, channels: usize) -> f32 {
    let mut silent_secs = 0.0f32;
    let window = WINDOW_SIZE.min(sample_rate);
    let frame = window * channels;
    let mut pos = samples.len();

    while pos >= frame {
        let start = pos - frame;
        let chunk = &samples[start..pos];
        let mono: Vec<f32> = if channels >= 2 {
            chunk.chunks(2).map(|c| (c[0] + c[1]) * 0.5).collect()
        } else {
            chunk.to_vec()
        };
        let db = rms_db(&mono);
        if db > threshold_db {
            break;
        }
        silent_secs += mono.len() as f32 / sample_rate as f32;
        pos = start;
    }
    silent_secs
}

/// Detect leading and trailing silence in a track.
/// Returns `(trim_start_secs, trim_end_secs)`.
///
/// Threshold used is -60 dBFS unless specified otherwise.
pub fn detect_silence(path: &Path, total_duration: f32) -> (f32, f32) {
    detect_silence_with_threshold(path, total_duration, DEFAULT_THRESHOLD_DB)
}

/// Same as `detect_silence` but with an explicit threshold.
pub fn detect_silence_with_threshold(
    path: &Path,
    total_duration: f32,
    threshold_db: f32,
) -> (f32, f32) {
    let alive = AtomicBool::new(true);

    // --- Scan leading silence ---
    let start_dur = MAX_SCAN_SECS.min(total_duration);
    let (start_samples, sr, ch) = match decode_region(path, 0.0, start_dur, &alive) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[SilenceDetect] start error: {e}");
            return (0.0, 0.0);
        }
    };
    let mut trim_start = scan_leading_silence(&start_samples, sr, threshold_db, ch);

    // --- Scan trailing silence ---
    let end_dur = MAX_SCAN_SECS.min((total_duration - trim_start).max(0.0));
    let seek_to = (total_duration - end_dur).max(0.0) as f64;
    let (end_samples, _, _) = match decode_region(path, seek_to, end_dur, &alive) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[SilenceDetect] end error: {e}");
            return (trim_start, 0.0);
        }
    };
    let raw_trailing = scan_trailing_silence(&end_samples, sr, threshold_db, ch);
    let mut trim_end = raw_trailing.min(end_dur);

    // Safety valves: if trim exceeds 95 % of total duration, reject
    if trim_start > total_duration * 0.95 {
        trim_start = 0.0;
    }
    if trim_end > total_duration * 0.95 {
        trim_end = 0.0;
    }

    // Ignore sub-50ms silence
    if trim_start < MIN_SILENCE_SECS && trim_end < MIN_SILENCE_SECS {
        (0.0, 0.0)
    } else {
        (trim_start, trim_end)
    }
}
