//! Leading and trailing silence detection for audio files.
//!
//! The detection pipeline:
//!
//! 1. Decode up to `MAX_SCAN_SECS` (15 s) from the file start using
//!    [Symphonia](https://github.com/pdeljanov/Symphonia).
//! 2. Split decoded samples into fixed-size windows and compute each
//!    window's RMS power in dBFS.
//! 3. Count consecutive silent windows as leading silence.
//! 4. Repeat from the file end for trailing silence, working backwards.
//! 5. Apply safety valves:
//!    * Reject trims exceeding 95 % of total duration
//!    * Ignore sub-`MIN_SILENCE_SECS` (50 ms) detections to avoid false
//!      positives on brief pauses.
//!
//! The default silence threshold is −60 dBFS.

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

/// Default silence threshold in dBFS.
///
/// Samples whose RMS level falls below this value are considered silent.
const DEFAULT_THRESHOLD_DB: f32 = -60.0;

/// Maximum duration (seconds) to decode and scan from the start / end of each track.
const MAX_SCAN_SECS: f32 = 15.0;

/// Minimum consecutive silence (seconds) required for a valid detection.
///
/// Silences shorter than this threshold are ignored to avoid false positives on
/// brief pauses in the audio signal.
const MIN_SILENCE_SECS: f32 = 0.05;

/// Number of samples per RMS measurement window.
///
/// Each window is independently evaluated against the silence threshold.
const WINDOW_SIZE: usize = 4096;

/// Compute the RMS power of a sample buffer in dBFS.
///
/// Returns −100 dB for a zero (or all-silent) buffer.
fn rms_db(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    if sum <= 0.0 {
        return -100.0;
    }
    let rms = (sum / samples.len() as f32).sqrt();
    20.0 * rms.log10()
}

/// Decode up to `duration_seconds` of audio starting at `seek_seconds` from the file.
///
/// # Returns
///
/// `(samples_interleaved, sample_rate, channels)`
///
/// Samples are interleaved in the native channel order of the file.  Decoding
/// is aborted early if `alive` is set to `false` or if the elapsed time exceeds
/// 30 seconds (safety timeout for corrupt files).
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

/// Scan forward through `samples` and return the duration (seconds) of
/// consecutive silence at the start.
///
/// The buffer is split into fixed-size RMS windows. Once a window exceeds
/// `threshold_db` the scan stops.
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

/// Scan backward through `samples` and return the duration (seconds) of
/// consecutive silence at the tail.
///
/// Works from the end of the buffer toward the beginning, stopping when a
/// non-silent window is found.
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

/// Detect leading and trailing silence in a track using the default threshold (−60 dBFS).
///
/// # Returns
///
/// `(trim_start_secs, trim_end_secs)` — seconds of silence to remove from
/// the start and end of the track respectively.
///
/// Both values are clamped to zero if they exceed 95 % of `total_duration`,
/// and the pair is zeroed out if both are below `MIN_SILENCE_SECS` (50 ms).
pub fn detect_silence(path: &Path, total_duration: f32) -> (f32, f32) {
    detect_silence_with_threshold(path, total_duration, DEFAULT_THRESHOLD_DB)
}

/// Detect leading and trailing silence with an explicit dBFS threshold.
///
/// See [`detect_silence`] for details on return values and safety valves.
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
