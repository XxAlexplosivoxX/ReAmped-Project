//! Shared decoding utilities used by every audio backend.
//!
//! [`decode_loop`] is the common decode thread: it opens a file with
//! Symphonia, decodes PCM into f32, and pushes interleaved stereo frames into
//! a lock-free ring-buffer producer. When the file's native sample rate equals
//! the requested output rate the resampler is skipped entirely, which is what
//! makes bit-perfect output possible.
//!
//! [`probe_audio`] opens and decodes the first packet of a file to report its
//! native audio properties (rate, channels, decoded buffer kind, bit depth).
//! The ALSA backend uses it to configure the hardware before playback.

use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, sleep},
    time::Duration,
};

use atomic_float::AtomicF32;
use audioadapter_buffers::number_to_float::InterleavedNumbers;
use ringbuf::Producer;
use rubato::{Fft, Resampler};

use symphonia::{
    core::{
        audio::{AudioBufferRef, SampleBuffer},
        codecs::DecoderOptions,
        formats::{FormatOptions, SeekMode, SeekTo},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_probe,
};

use super::viz_source::SharedSamples;

/// Size of the ring buffer (in stereo frames) for each decode thread.
/// At 48 kHz this gives ≈ 4 seconds of buffer.
pub(crate) const RING_CAPACITY_FRAMES: usize = 192_000;

/// The f32 sample container kind produced by the decoder for a file.
///
/// Determines the inverse scaling applied when converting f32 samples back to
/// the native integer format for bit-perfect output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedBufferKind {
    /// 16-bit signed integer, f32 scale `2^15`.
    S16,
    /// 24-bit signed integer, f32 scale `2^23`.
    S24,
    /// 32-bit signed integer container, f32 scale `2^31` (also produced for
    /// FLAC files of any bit depth; the true precision is in `bits_per_sample`).
    S32,
    /// 8-bit unsigned integer, f32 scale `2^7`.
    U8,
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
}

/// Native audio properties of a file, as reported by a decode probe.
#[derive(Debug, Clone)]
pub struct FileAudio {
    /// Native sample rate in Hz.
    pub sample_rate: usize,
    /// Channel count.
    pub channels: usize,
    /// Decoded buffer kind (drives the f32 → native conversion).
    pub kind: DecodedBufferKind,
    /// Nominal bit depth reported by the codec parameters.
    pub bits_per_sample: u32,
}

/// Probe a file and return its native audio properties.
///
/// Opens the file, probes the format with Symphonia, creates a decoder, and
/// decodes the first packet to determine the actual sample container kind.
pub fn probe_audio(path: &Path) -> Result<FileAudio, String> {
    let file = File::open(path).map_err(|e| format!("failed to open '{}': {}", path.display(), e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("probe failed '{}': {e:?}", path.display()))?;

    let mut format = probed.format;
    let track = format.default_track().ok_or_else(|| "no default track".to_string())?;
    let params = &track.codec_params;

    let sample_rate = params.sample_rate.ok_or_else(|| "no sample rate".to_string())? as usize;
    let channels = params.channels.map(|c| c.count()).unwrap_or(2);
    let bits_per_sample = params.bits_per_sample.unwrap_or(16);

    let mut decoder = symphonia::default::get_codecs()
        .make(params, &DecoderOptions::default())
        .map_err(|e| format!("decoder error: {e:?}"))?;

    let kind = loop {
        let packet = format.next_packet().map_err(|e| format!("no decodable packet: {e:?}"))?;
        match decoder.decode(&packet) {
            Ok(decoded) => break classify_buffer(&decoded),
            Err(_) => continue,
        }
    };

    Ok(FileAudio { sample_rate, channels, kind, bits_per_sample })
}

/// Map a decoded buffer to its [`DecodedBufferKind`].
fn classify_buffer(decoded: &AudioBufferRef<'_>) -> DecodedBufferKind {
    match decoded {
        AudioBufferRef::S16(_) => DecodedBufferKind::S16,
        AudioBufferRef::S24(_) => DecodedBufferKind::S24,
        AudioBufferRef::S32(_) => DecodedBufferKind::S32,
        AudioBufferRef::U8(_) => DecodedBufferKind::U8,
        AudioBufferRef::F32(_) => DecodedBufferKind::F32,
        AudioBufferRef::F64(_) => DecodedBufferKind::F64,
        _ => DecodedBufferKind::S16,
    }
}

/// Shared decode loop (runs on the backend's decode thread).
///
/// Opens the file at `path`, probes the format with Symphonia, creates a
/// decoder, seeks to `seek_seconds`, and decodes audio frames into the
/// `producer` ring buffer as interleaved stereo f32.
///
/// * If `output_sr` equals the file's native sample rate the resampler is
///   skipped entirely (bit-perfect passthrough).
/// * If they differ, resampling to `output_sr` is done via rubato.
///
/// The loop exits when the file ends, the effective-duration frame limit is
/// reached, or `alive` becomes false. The `playing` flag allows the caller to
/// pause decoding without killing the thread (the decoder sleeps instead of
/// busy-waiting).
///
/// `effective_duration_secs` bounds the number of *output* frames decoded
/// (used for silence trimming); `f32::INFINITY` means unbounded.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_loop(
    path: &Path,
    output_sr: usize,
    mut producer: Producer<f32>,
    alive: &AtomicBool,
    playing: &AtomicBool,
    finished: &AtomicBool,
    effective_duration_secs: &AtomicF32,
    seek_seconds: f32,
    _samples_viz: SharedSamples,
) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[Decode] failed to open '{}': {}", path.display(), e);
            finished.store(true, Ordering::SeqCst);
            return;
        }
    };

    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = match get_probe().format(
        &Hint::new(),
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[Decode] probe failed '{}': {:?}", path.display(), e);
            finished.store(true, Ordering::SeqCst);
            return;
        }
    };

    let mut format = probed.format;
    let track = match format.default_track() {
        Some(t) => t,
        None => {
            eprintln!("[Decode] no default track in '{}'", path.display());
            finished.store(true, Ordering::SeqCst);
            return;
        }
    };

    let channels = match track.codec_params.channels {
        Some(c) => c.count(),
        None => {
            finished.store(true, Ordering::SeqCst);
            return;
        }
    };

    let input_sr = match track.codec_params.sample_rate {
        Some(sr) => sr as usize,
        None => {
            finished.store(true, Ordering::SeqCst);
            return;
        }
    };

    let mut params = track.codec_params.clone();
    params.sample_rate = Some(output_sr as u32);

    let mut decoder = match symphonia::default::get_codecs().make(&params, &DecoderOptions::default())
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[Decode] decoder error: {e:?}");
            finished.store(true, Ordering::SeqCst);
            return;
        }
    };

    // Seek to requested position
    if seek_seconds > 0.0 {
        let _ = format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: seek_seconds.into(),
                track_id: Some(track.id),
            },
        );
    }

    // Bit-perfect passthrough: no resampler when the rates match.
    let needs_resample = input_sr != output_sr;
    let chunk_size = 128;
    let mut resampler = if needs_resample {
        Some(
            Fft::<f32>::new(
                input_sr, output_sr, chunk_size, 2, channels,
                rubato::FixedSync::Output,
            )
            .unwrap(),
        )
    } else {
        None
    };

    let mut frames_pushed: u32 = 0;
    let mut interleaved = Vec::<f32>::new();

    while alive.load(Ordering::SeqCst) {
        // Stop decoding when we've pushed enough frames (silence trim).
        // The limit is re-derived from the effective duration each iteration
        // so it can be updated mid-decode (e.g. after a crossfade swap).
        let max_frame_limit =
            (effective_duration_secs.load(Ordering::Relaxed) * output_sr as f32) as u32;
        if frames_pushed >= max_frame_limit {
            break;
        }

        // When paused (primary thread only), sleep to save CPU
        if !playing.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
            continue;
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

        for frame_samples in buf.samples().chunks(channels) {
            let (l, r) = if channels == 1 {
                (frame_samples[0], frame_samples[0])
            } else {
                (frame_samples[0], frame_samples[1])
            };
            interleaved.push(l);
            interleaved.push(r);

            if needs_resample {
                if let Some(rs) = &mut resampler {
                    let needed = rs.input_frames_next();
                    if interleaved.len() >= needed * 2 {
                        let input =
                            InterleavedNumbers::new(&interleaved[..needed * 2], 2, needed).unwrap();
                        let output = rs.process(&input, 0, None).unwrap();
                        let out = output.take_data();

                        // Wait for space in ring buffer
                        while alive.load(Ordering::SeqCst)
                            && producer.len() + out.len() > producer.capacity()
                        {
                            thread::sleep(Duration::from_millis(1));
                        }
                        if !alive.load(Ordering::SeqCst) {
                            break;
                        }

                        if !push_bounded(
                            &mut producer,
                            &out,
                            &mut frames_pushed,
                            max_frame_limit,
                        ) {
                            break;
                        }

                        interleaved.drain(..needed * 2);
                    }
                }
            } else if interleaved.len() >= chunk_size * 2 {
                // Passthrough: push decoded frames directly, no resampling.
                while alive.load(Ordering::SeqCst)
                    && producer.len() + interleaved.len() > producer.capacity()
                {
                    thread::sleep(Duration::from_millis(1));
                }
                if !alive.load(Ordering::SeqCst) {
                    break;
                }
                if !push_bounded(
                    &mut producer,
                    &interleaved,
                    &mut frames_pushed,
                    max_frame_limit,
                ) {
                    break;
                }
                interleaved.clear();
            }
            if interleaved.capacity() > 8192 {
                interleaved.shrink_to(4096);
            }
        }
    }

    // Flush remaining samples through resampler (or directly)
    while alive.load(Ordering::SeqCst) && !interleaved.is_empty() {
        let max_frame_limit =
            (effective_duration_secs.load(Ordering::Relaxed) * output_sr as f32) as u32;

        let out: Vec<f32> = if needs_resample {
            if let Some(rs) = &mut resampler {
                let needed = rs.input_frames_next();
                if needed == 0 {
                    break;
                }
                let mut buf_in = interleaved.clone();
                if buf_in.len() < needed * 2 {
                    buf_in.resize(needed * 2, 0.0);
                }
                let input = match InterleavedNumbers::new(&buf_in[..needed * 2], 2, needed) {
                    Ok(i) => i,
                    Err(_) => break,
                };
                let output = match rs.process(&input, 0, None) {
                    Ok(o) => o,
                    Err(_) => break,
                };
                let resampled = output.take_data();
                if interleaved.len() >= needed * 2 {
                    interleaved.drain(..needed * 2);
                } else {
                    interleaved.clear();
                }
                resampled
            } else {
                break;
            }
        } else {
            std::mem::take(&mut interleaved)
        };

        while alive.load(Ordering::SeqCst)
            && producer.len() + out.len() > producer.capacity()
        {
            thread::sleep(Duration::from_millis(1));
        }
        if !alive.load(Ordering::SeqCst) {
            break;
        }
        if !out.is_empty() && !push_bounded(&mut producer, &out, &mut frames_pushed, max_frame_limit)
        {
            break;
        }
    }

    // Let consumer drain before signalling finished
    sleep(Duration::from_millis(100));
    finished.store(true, Ordering::SeqCst);
}

/// Push `samples` into the producer, respecting the `max_frame_limit`.
///
/// Returns `false` when the limit has been reached (the caller should stop).
fn push_bounded(
    producer: &mut Producer<f32>,
    samples: &[f32],
    frames_pushed: &mut u32,
    max_frame_limit: u32,
) -> bool {
    let n_frames = samples.len() / 2;
    if *frames_pushed + n_frames as u32 > max_frame_limit {
        let allowed = (max_frame_limit - *frames_pushed) as usize * 2;
        if allowed > 0 {
            let _ = producer.push_slice(&samples[..allowed]);
        }
        return false;
    }
    let _ = producer.push_slice(samples);
    *frames_pushed += n_frames as u32;
    true
}
