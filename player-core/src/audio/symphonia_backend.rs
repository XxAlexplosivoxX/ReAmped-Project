//! Symphonia-based CPAL audio backend.
//!
//! [`SymphoniaBackend`] implements the [`AudioBackend`] trait using the
//! Symphonia decoding library for format-agnostic audio decoding and CPAL
//! for cross-platform audio output. Decode runs in a dedicated thread that
//! feeds a lock-free ring buffer; the CPAL output callback drains that buffer
//! and applies DSP (EQ, stereo widening, volume, crossfade mixing).
//!
//! This is the default (fallback) backend used when bit-perfect mode is
//! disabled, unsupported by the hardware, or when the device is busy.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
    time::Instant,
};

use atomic_float::AtomicF32;
use cpal::{
    Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{Consumer, RingBuffer};

use super::{
    AudioBackend, BackendError, crossfade,
    decode::{self, RING_CAPACITY_FRAMES},
    viz_source::SharedSamples,
};
use crate::{
    Track,
    config::load_config,
    dsp::{db_meter::DbMeter, mini_eq::TripleBandEq, xpander::Expander},
};

// ---------------------------------------------------------------------------
// SymphoniaBackend
// ---------------------------------------------------------------------------

/// Audio backend using Symphonia (decode) + CPAL (output).
///
/// Drives decoding in a separate thread, feeding a lock-free ring buffer
/// that the real-time CPAL output callback drains. All DSP parameters
/// (volume, EQ, expander, crossfade gains) are stored as atomics so the
/// callback can read them without locking.
///
/// ## Fields
///
/// * `samples` — shared visualisation buffer written by the output callback
/// * `playing`, `alive`, `finished` — decode-thread lifecycle flags
/// * `start`, `paused_at` — cumulative position tracking
/// * `volume`, `low_gain`, `mid_gain`, `high_gain`, `expander_width` — DSP atomics (stored as u32 scaled by 100)
/// * `db_meter_l`, `db_meter_r` — per-channel loudness meters
/// * `fade_state`, `fade_start`, `fade_duration_ms`, `paused_during_fade` — play/pause fade control
/// * `stream` — the active CPAL output stream
/// * `decode_handle` — join handle for the primary decode thread
/// * `next_alive`, `next_finished`, `next_decode_handle`, `next_path` — next-track (crossfade) decode resources
/// * `xfade_*` — crossfade state atomics
/// * `primary_consumer`, `xfade_consumer` — ring-buffer consumers hot-swapped during crossfade
/// * `trim_start`, `trim_end`, `max_output_frames` — silence trimming configuration
pub struct SymphoniaBackend {
    samples: SharedSamples,
    playing: Arc<AtomicBool>,
    start: Option<Instant>,
    paused_at: f32,
    pause_fade_started: Option<Instant>,
    volume: Arc<AtomicU32>,
    stream: Option<Stream>,
    alive: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    decode_handle: Option<thread::JoinHandle<()>>,

    // Next track
    next_alive: Arc<AtomicBool>,
    next_finished: Arc<AtomicBool>,
    next_decode_handle: Option<thread::JoinHandle<()>>,
    next_path: Option<PathBuf>,

    // DSP shared
    low_gain: Arc<AtomicU32>,
    mid_gain: Arc<AtomicU32>,
    high_gain: Arc<AtomicU32>,
    expander_width: Arc<AtomicU32>,
    db_meter_l: Arc<Mutex<DbMeter>>,
    db_meter_r: Arc<Mutex<DbMeter>>,
    fade_state: Arc<AtomicU32>,
    fade_start: Arc<Mutex<Option<Instant>>>,
    fade_duration_ms: Arc<AtomicU32>,
    paused_during_fade: Arc<AtomicBool>,
    sample_rate: f32,

    // Crossfade
    xfade_active: Arc<AtomicBool>,
    xfade_out_gain: Arc<AtomicF32>,
    xfade_in_gain: Arc<AtomicF32>,
    xfade_duration_ms: Arc<AtomicU32>,
    xfade_start: Arc<Mutex<Option<Instant>>>,
    xfade_micro_frame: Arc<AtomicU32>,

    // Shared consumers for CPAL callback (hot-swap via Arc<Mutex<Option<...>>>)
    primary_consumer: Arc<Mutex<Option<Consumer<f32>>>>,
    xfade_consumer: Arc<Mutex<Option<Consumer<f32>>>>,

    // Silence trim
    trim_start: Arc<AtomicF32>,
    trim_end: Arc<AtomicF32>,
    effective_duration_secs: Arc<AtomicF32>,
    next_effective_duration_secs: Arc<AtomicF32>,
}

impl SymphoniaBackend {
    /// Construct a new [`SymphoniaBackend`].
    ///
    /// `samples` is a shared buffer that the CPAL output callback fills with
    /// interleaved stereo frames for waveform visualisation. All other fields
    /// are initialised to their default / idle state.
    pub fn new(samples: SharedSamples) -> Self {
        Self {
            samples,
            playing: Arc::new(AtomicBool::new(false)),
            start: None,
            paused_at: 0.0,
            pause_fade_started: None,
            volume: Arc::new(AtomicU32::new((load_config().volume * 100.0) as u32)),
            stream: None,
            alive: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
            decode_handle: None,
            next_alive: Arc::new(AtomicBool::new(false)),
            next_finished: Arc::new(AtomicBool::new(false)),
            next_decode_handle: None,
            next_path: None,
            low_gain: Arc::new(AtomicU32::new(100)),
            mid_gain: Arc::new(AtomicU32::new(100)),
            high_gain: Arc::new(AtomicU32::new(100)),
            expander_width: Arc::new(AtomicU32::new(100)),
            db_meter_l: Arc::new(Mutex::new(DbMeter::new())),
            db_meter_r: Arc::new(Mutex::new(DbMeter::new())),
            fade_state: Arc::new(AtomicU32::new(0)),
            fade_start: Arc::new(Mutex::new(None)),
            fade_duration_ms: Arc::new(AtomicU32::new(350)),
            paused_during_fade: Arc::new(AtomicBool::new(false)),
            sample_rate: 41000.0,
            xfade_active: Arc::new(AtomicBool::new(false)),
            xfade_out_gain: Arc::new(AtomicF32::new(1.0)),
            xfade_in_gain: Arc::new(AtomicF32::new(0.0)),
            xfade_duration_ms: Arc::new(AtomicU32::new(0)),
            xfade_start: Arc::new(Mutex::new(None)),
            xfade_micro_frame: Arc::new(AtomicU32::new(0)),
            primary_consumer: Arc::new(Mutex::new(None)),
            xfade_consumer: Arc::new(Mutex::new(None)),
            trim_start: Arc::new(AtomicF32::new(0.0)),
            trim_end: Arc::new(AtomicF32::new(0.0)),
            effective_duration_secs: Arc::new(AtomicF32::new(f32::INFINITY)),
            next_effective_duration_secs: Arc::new(AtomicF32::new(f32::INFINITY)),
        }
    }

    // -----------------------------------------------------------------------
    // Internal: spawn decode thread + CPAL output stream
    // -----------------------------------------------------------------------
    //
    // Starts a Symphonia decode thread that feeds `primary_consumer`, then
    // builds a CPAL output stream whose callback drains that consumer, applies
    // DSP (EQ, stereo expander, volume, crossfade mixing), and writes samples
    // to the visualisation shared buffer.
    //
    // `seek_seconds` — seek the decoder to this absolute position before playback.
    fn spawn_player(&mut self, path: &Path, seek_seconds: f32) {
        self.alive.store(true, Ordering::SeqCst);
        self.finished.store(false, Ordering::SeqCst);

        let samples_viz = self.samples.clone();
        let samples_viz_decode = samples_viz.clone();
        let playing = self.playing.clone();
        let volume = self.volume.clone();

        playing.store(true, Ordering::SeqCst);
        self.start = Some(Instant::now());

        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let mut config: cpal::StreamConfig = device.default_output_config().unwrap().into();
        config.channels = 2;
        let output_sr = config.sample_rate as usize;
        self.sample_rate = output_sr as f32;

        let ring = RingBuffer::<f32>::new(RING_CAPACITY_FRAMES * 2);
        let (producer, consumer) = ring.split();
        *self.primary_consumer.lock().unwrap() = Some(consumer);

        let finished = self.finished.clone();
        let path = path.to_owned();
        let pl = playing.clone();
        let alive_cl = self.alive.clone();

        // Crossfade consumer
        let primary_consumer = self.primary_consumer.clone();
        let xfade_consumer = self.xfade_consumer.clone();
        let xfade_active = self.xfade_active.clone();
        let xfade_out = self.xfade_out_gain.clone();
        let xfade_in = self.xfade_in_gain.clone();
        let xfade_micro = self.xfade_micro_frame.clone();
        let effective_dur = self.effective_duration_secs.clone();

        let decode_handle = thread::spawn(move || {
            decode::decode_loop(
                &path, output_sr, producer, &alive_cl, &pl, &finished,
                &effective_dur, seek_seconds, samples_viz_decode,
            );
        });
        self.decode_handle = Some(decode_handle);

        // ---- CPAL output stream ----
        let mut eq_l = TripleBandEq::new();
        let mut eq_r = TripleBandEq::new();
        let mut eq_xfade_l = TripleBandEq::new();
        let mut eq_xfade_r = TripleBandEq::new();
        let mut expander_stereo = Expander::new();

        let low_g = self.low_gain.clone();
        let mid_g = self.mid_gain.clone();
        let high_g = self.high_gain.clone();
        let width = self.expander_width.clone();
        let db_meter_l = self.db_meter_l.clone();
        let db_meter_r = self.db_meter_r.clone();
        let fade_state = self.fade_state.clone();
        let fade_start = self.fade_start.clone();
        let fade_duration_ms = self.fade_duration_ms.clone();
        let paused_during_fade = self.paused_during_fade.clone();
        let sample_rate_f = output_sr as f32;

        // Micro-fade constants
        let micro_fade_total = crossfade::micro_fade_frame_count(sample_rate_f);

        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    let state = fade_state.load(Ordering::Relaxed);
                    let is_playing = playing.load(Ordering::Relaxed);
                    let is_paused = paused_during_fade.load(Ordering::Relaxed);
                    let has_fade = state != 0;
                    let xfade = xfade_active.load(Ordering::Relaxed);

                    if !is_playing && !has_fade && !xfade {
                        out.fill(0.0);
                        return;
                    }

                    // ---- Play/pause fade ----
                    let mut fade_mult = 1.0f32;
                    if (state == 1 || state == 2) && let Ok(Some(start)) = fade_start.lock().map(|f| *f) {
                        let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                        let fade_dur = fade_duration_ms.load(Ordering::Relaxed) as f32;
                        if elapsed_ms >= fade_dur {
                            if state == 1 {
                                fade_mult = 1.0;
                            } else {
                                fade_mult = 0.0;
                            }
                            fade_state.store(0, Ordering::Relaxed);
                            if is_paused && state == 2 {
                                playing.store(false, Ordering::Relaxed);
                            }
                        } else {
                            fade_mult = if state == 1 {
                                elapsed_ms / fade_dur
                            } else {
                                1.0 - elapsed_ms / fade_dur
                            };
                        }
                    }

                    // ---- Read crossfade gains (pre-computed by player thread) ----
                    let xf_g_out = xfade_out.load(Ordering::Relaxed);
                    let xf_g_in = xfade_in.load(Ordering::Relaxed);

                    // ---- Micro-fade for incoming track ----
                    let micro_extra = if xfade {
                        crossfade::advance_micro_fade(&xfade_micro, micro_fade_total)
                    } else {
                        1.0
                    };

                    let vol = volume.load(Ordering::Relaxed) as f32 / 100.0 * fade_mult;
                    let g_l = low_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_m = mid_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_h = high_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let w = width.load(Ordering::Relaxed) as f32 / 100.0;
                    expander_stereo.set_width(w);

                    eq_l.update_all(g_l, g_m, g_h, sample_rate_f);
                    eq_r.update_all(g_l, g_m, g_h, sample_rate_f);
                    eq_xfade_l.update_all(g_l, g_m, g_h, sample_rate_f);
                    eq_xfade_r.update_all(g_l, g_m, g_h, sample_rate_f);

                    // Track per-frame samples for viz + meters
                    let mut left_samples = Vec::with_capacity(out.len() / 2);
                    let mut right_samples = Vec::with_capacity(out.len() / 2);
                    let mut viz = samples_viz.lock().unwrap();

                    // Guard: clear viz on pause completion
                    if state == 2 && fade_mult <= 0.001 {
                        viz.clear();
                    }

                    for frame in out.chunks_mut(2) {
                        // ---- Read primary (current) track ----
                        let (s_l, s_r) = {
                            let mut pc = primary_consumer.lock().unwrap();
                            if let Some(ref mut c) = *pc {
                                (c.pop().unwrap_or(0.0), c.pop().unwrap_or(0.0))
                            } else {
                                (0.0, 0.0)
                            }
                        };

                        let eq_l_out = eq_l.process(s_l);
                        let eq_r_out = eq_r.process(s_r);
                        let (final_l, final_r) =
                            expander_stereo.process_stereo_width(eq_l_out, eq_r_out);

                        if xfade {
                            // ---- Read next (crossfade) track ----
                            let (n_l, n_r) = {
                                let mut xc = xfade_consumer.lock().unwrap();
                                if let Some(ref mut nc) = *xc {
                                    (nc.pop().unwrap_or(0.0), nc.pop().unwrap_or(0.0))
                                } else {
                                    (0.0, 0.0)
                                }
                            };
                            let eq_nl = eq_xfade_l.process(n_l);
                            let eq_nr = eq_xfade_r.process(n_r);
                            let (fn_l, fn_r) =
                                expander_stereo.process_stereo_width(eq_nl, eq_nr);

                            // Mix with crossfade gains + micro-fade on incoming
                            frame[0] = (final_l * xf_g_out + fn_l * xf_g_in * micro_extra) * vol;
                            frame[1] = (final_r * xf_g_out + fn_r * xf_g_in * micro_extra) * vol;
                        } else {
                            frame[0] = final_l * vol;
                            frame[1] = final_r * vol;
                        }

                        left_samples.push(frame[0]);
                        right_samples.push(frame[1]);
                        viz.push(frame[0]);
                        viz.push(frame[1]);
                    }

                    if let Ok(mut m) = db_meter_l.try_lock() {
                        m.process_buffer(&left_samples);
                    }
                    if let Ok(mut m) = db_meter_r.try_lock() {
                        m.process_buffer(&right_samples);
                    }
                },
                |e| eprintln!("[CPAL] audio error: {e}"),
                None,
            )
            .unwrap();

        stream.play().unwrap();
        self.stream = Some(stream);
    }
}
// ===========================================================================
// AudioBackend trait implementation
// ===========================================================================

impl AudioBackend for SymphoniaBackend {
    /// Load a track: stop current playback, abort any crossfade, and spawn a
    /// new decode thread plus CPAL output stream. The decoder seeks to
    /// `trim_start` so silence at the beginning of the file is skipped.
    fn load(&mut self, track: &Track) -> Result<(), BackendError> {
        self.stop();
        self.crossfade_abort();
        let seek = self.trim_start.load(Ordering::SeqCst);
        self.spawn_player(&track.path, seek);
        Ok(())
    }

    /// Start or resume playback with a short fade-in (state = 1).
    fn play(&mut self) {
        if let Some(start) = self.start {
            // Still had a live window (e.g. re-play while playing): fold it.
            self.paused_at += start.elapsed().as_secs_f32();
        } else if let Some(fade_start) = self.pause_fade_started {
            // Resume after a pause: only the fade-out portion actually
            // sounded, so don't fold the whole paused time in.
            let fade_dur = self.fade_duration_ms.load(Ordering::Relaxed) as f32 / 1000.0;
            self.paused_at += fade_start.elapsed().as_secs_f32().min(fade_dur);
        }
        self.pause_fade_started = None;
        self.start = Some(Instant::now());
        self.playing.store(true, Ordering::SeqCst);
        self.paused_during_fade.store(false, Ordering::SeqCst);
        self.fade_state.store(1, Ordering::SeqCst);
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    /// Pause playback and freeze the position. A fade-out is triggered
    /// (state = 2); once complete the playing flag is cleared by the callback.
    /// Visualisation samples are cleared immediately.
    ///
    /// If a crossfade was active, its mixing flag is cleared immediately so
    /// the CPAL callback outputs only the primary track's fade-out. The
    /// next-track ring buffer is preserved (the decode thread sleeps when
    /// `playing` becomes false after the fade), so [`play()`] can resume the
    /// crossfade from the frozen [`PausedFading`](crate::audio::crossfade::CrossfadePhase::PausedFading) gains.
    fn pause(&mut self) {
        // Fold the time that sounded so far into the base, then start a new
        // window for the fade-out. The fade-out is still audible, so
        // `position()` keeps advancing from `pause_fade_started` until the
        // audio goes silent and then holds. `play()` folds that advance on
        // resume, keeping the displayed position in sync with what was heard.
        if let Some(start) = self.start {
            self.paused_at += start.elapsed().as_secs_f32();
            self.start = None;
        }
        if !self.paused_during_fade.load(Ordering::SeqCst) {
            self.pause_fade_started = Some(Instant::now());
        }
        self.samples.lock().unwrap().clear();
        // Stop crossfade mixing — the callback will only process the primary
        // track's fade-out. The next-track consumer is kept alive so the
        // decode thread can sleep in place and resume on play().
        self.xfade_active.store(false, Ordering::SeqCst);
        self.xfade_out_gain.store(0.0, Ordering::Relaxed);
        self.xfade_in_gain.store(0.0, Ordering::Relaxed);
        self.xfade_micro_frame.store(0, Ordering::SeqCst);
        self.paused_during_fade.store(true, Ordering::SeqCst);
        self.fade_state.store(2, Ordering::SeqCst);
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    /// Fully stop playback: abort crossfade, kill decode threads, drop the
    /// CPAL stream, clear visualisation samples, and reset position.
    fn stop(&mut self) {
        self.crossfade_abort();
        self.next_alive.store(false, Ordering::SeqCst);
        self.playing.store(false, Ordering::SeqCst);
        self.alive.store(false, Ordering::SeqCst);
        self.start = None;
        self.paused_at = 0.0;
        self.pause_fade_started = None;
        self.paused_during_fade.store(false, Ordering::SeqCst);
        self.finished.store(false, Ordering::SeqCst);
        self.primary_consumer.lock().unwrap().take();
        if let Some(h) = self.decode_handle.take() {
            let _ = h.join();
        }
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        self.samples.lock().unwrap().clear();
    }

    /// Seek to `seconds` (relative, after trim) and restart playback.
    /// The absolute seek position is `seconds + trim_start`.
    fn seek(&mut self, path: &Path, seconds: f32) {
        self.crossfade_abort();
        self.stop();
        let ts = self.trim_start.load(Ordering::SeqCst);
        let raw_seek = seconds + ts;
        self.paused_at = raw_seek;
        self.spawn_player(path, raw_seek);
    }

    /// Current playback position in seconds (trim-relative).
    ///
    /// While a pause fade-out is still audible the position keeps advancing
    /// from the pause press, then holds once the audio is silent.
    fn position(&self) -> f32 {
        let ts = self.trim_start.load(Ordering::SeqCst);
        let abs = match self.start {
            Some(t) => self.paused_at + t.elapsed().as_secs_f32(),
            None => match self.pause_fade_started {
                // Paused: the fade-out was (or still is) audible, so advance
                // from the pause press up to the fade duration, then hold.
                Some(fade_start) => {
                    let fade_dur = self.fade_duration_ms.load(Ordering::Relaxed) as f32 / 1000.0;
                    self.paused_at + fade_start.elapsed().as_secs_f32().min(fade_dur)
                }
                None => self.paused_at,
            },
        };
        (abs - ts).max(0.0)
    }

    /// Whether audio is currently being output (true during a pause fade-out,
    /// false once the fade completes and the output is silent).
    fn is_audible(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    /// The output sample rate determined by the CPAL device.
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Shared buffer filled by the output callback with interleaved stereo frames.
    fn samples(&self) -> SharedSamples {
        self.samples.clone()
    }

    /// Whether the primary decode thread has reached end-of-file.
    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    /// Whether the primary consumer is empty *after* the decoder has finished.
    ///
    /// Returns `false` if the decoder is still running; only signals depletion
    /// once EOF is reached and the ring buffer has been fully drained.
    fn consumer_depleted(&self) -> bool {
        if !self.finished.load(Ordering::SeqCst) {
            return false;
        }
        let pc = self.primary_consumer.lock().unwrap();
        pc.as_ref().is_none_or(|c| c.is_empty())
    }

    /// Instantaneous loudness in dB for the right and left channels.
    fn get_db_loudness(&self) -> (f32, f32) {
        (
            self.db_meter_r.lock().unwrap().current_db,
            self.db_meter_l.lock().unwrap().current_db,
        )
    }

    /// Set master volume (0.0 – 1.0). Stored as u32 scaled by 100.
    fn set_volume(&self, volume: f32) {
        self.volume.store((volume * 100.0) as u32, Ordering::SeqCst);
    }

    /// Low-shelf EQ gain (0.0 – 2.0). Stored as u32 scaled by 100.
    fn low_gain(&self, gain: f32) {
        self.low_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    /// Mid-band EQ gain (0.0 – 2.0). Stored as u32 scaled by 100.
    fn mid_gain(&self, gain: f32) {
        self.mid_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    /// High-shelf EQ gain (0.0 – 2.0). Stored as u32 scaled by 100.
    fn high_gain(&self, gain: f32) {
        self.high_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    /// Set stereo expander width (0.0 = mono, 1.0 = original). Stored as u32 scaled by 100.
    fn set_expander_width(&self, width: f32) {
        self.expander_width.store((width * 100.0) as u32, Ordering::SeqCst);
    }

    /// Configure silence trimming.
    ///
    /// * `start_secs` — seconds to skip from the beginning of the file
    /// * `end_secs` — seconds to trim from the end (not directly used here)
    /// * `effective_duration_secs` — maximum output duration to decode,
    ///   scaled by the output sample rate to bound the frame count
    fn set_trim(&mut self, start_secs: f32, end_secs: f32, effective_duration_secs: f32) {
        self.trim_start.store(start_secs, Ordering::SeqCst);
        self.trim_end.store(end_secs, Ordering::SeqCst);
        self.effective_duration_secs.store(effective_duration_secs, Ordering::SeqCst);
    }

    /// Start trim offset in seconds.
    fn trim_start(&self) -> f32 {
        self.trim_start.load(Ordering::SeqCst)
    }

    /// End trim offset in seconds.
    fn trim_end(&self) -> f32 {
        self.trim_end.load(Ordering::SeqCst)
    }

    // ---- Crossfade primitives ----

    /// Start decoding the next track into a secondary ring buffer.
    ///
    /// The next-track decode thread uses its own lifecycle flags
    /// (`next_alive` / `next_finished`) and feeds `xfade_consumer`.
    /// The consumer is hot-swapped to `primary_consumer` when the
    /// crossfade completes (see [`crossfade_swap`](Self::crossfade_swap)).
    fn prepare_next(&mut self, path: &Path, trim_start: f32) -> bool {
        self.crossfade_abort();
        self.next_alive.store(true, Ordering::SeqCst);
        self.next_finished.store(false, Ordering::SeqCst);
        self.next_effective_duration_secs.store(f32::INFINITY, Ordering::SeqCst);

        let output_sr = self.sample_rate as usize;
        let ring = RingBuffer::<f32>::new(RING_CAPACITY_FRAMES * 2);
        let (producer, consumer) = ring.split();
        *self.xfade_consumer.lock().unwrap() = Some(consumer);

        let alive = self.next_alive.clone();
        let finished_out = self.next_finished.clone();
        let path_for_closure = path.to_owned();
        let path_for_store = path.to_owned();

        // For the next-track decode we use a dummy finished/playing atomics
        // that keep the thread alive until explicitly killed.
        let dummy_playing = Arc::new(AtomicBool::new(true));
        let dummy_finished = finished_out.clone();
        let effective_dur_dummy = self.next_effective_duration_secs.clone();
        let samples_dummy = Arc::new(Mutex::new(Vec::new()));

        let handle = thread::spawn(move || {
            decode::decode_loop(
                &path_for_closure,
                output_sr,
                producer,
                &alive,
                &dummy_playing,
                &dummy_finished,
                &effective_dur_dummy,
                trim_start,
                samples_dummy,
            );
        });

        self.next_decode_handle = Some(handle);
        self.next_path = Some(path_for_store);
        true
    }

    /// Begin the crossfade in the CPAL callback.
    ///
    /// Sets the initial gains (out = 1.0, in = 0.0), records the start
    /// instant, resets the micro-fade counter, and activates the crossfade.
    /// The gains are updated externally by the player thread.
    fn start_crossfade(&mut self, duration_ms: u32) {
        self.xfade_out_gain.store(1.0, Ordering::SeqCst);
        self.xfade_in_gain.store(0.0, Ordering::SeqCst);
        self.xfade_duration_ms.store(duration_ms, Ordering::SeqCst);
        *self.xfade_start.lock().unwrap() = Some(Instant::now());
        crossfade::reset_micro_fade(&self.xfade_micro_frame);
        self.xfade_active.store(true, Ordering::SeqCst);
    }

    /// Whether the CPAL callback is currently crossfade-mixing two tracks.
    fn is_crossfade_active(&self) -> bool {
        self.xfade_active.load(Ordering::SeqCst)
    }

    /// Whether the next-track decode thread has reached end-of-file.
    fn is_next_finished(&self) -> bool {
        self.next_finished.load(Ordering::SeqCst)
    }

    /// Current crossfade gains `(out_gain, in_gain)` as set by the player thread.
    fn crossfade_gains(&self) -> (f32, f32) {
        (
            self.xfade_out_gain.load(Ordering::SeqCst),
            self.xfade_in_gain.load(Ordering::SeqCst),
        )
    }

    /// Re-activate crossfade mixing in the CBAL callback after a pause.
    ///
    /// Gains must be restored first via [`set_crossfade_gains`]; this only
    /// flags `xfade_active` back to `true` so the callback's mixing branch
    /// is entered on the next invocation.
    fn resume_crossfade(&self) {
        self.xfade_active.store(true, Ordering::SeqCst);
    }

    /// Override crossfade gains (used when resuming from a pause during fade).
    fn set_crossfade_gains(&self, out: f32, in_: f32) {
        self.xfade_out_gain.store(out, Ordering::SeqCst);
        self.xfade_in_gain.store(in_, Ordering::SeqCst);
    }

    /// Promote the next-track to primary.
    ///
    /// Kills the old decode thread, swaps the crossfade consumer into the
    /// primary slot, promotes the next decode handle, resets all crossfade
    /// state, and adjusts the position counter to account for seconds that
    /// have already played during the crossfade (`xf_elapsed`).
    ///
    /// Returns the path of the newly-promoted track.
    fn crossfade_swap(&mut self, xf_elapsed: f32) -> Option<PathBuf> {
        let path = self.next_path.take();
        let next_decode = self.next_decode_handle.take();

        // Kill old decode thread
        self.alive.store(false, Ordering::SeqCst);
        if let Some(h) = self.decode_handle.take() {
            let _ = h.join();
        }

        self.finished.store(false, Ordering::SeqCst);

        // Move xfade_consumer -> primary_consumer
        let xf = self.xfade_consumer.lock().unwrap().take();
        if let Some(xc) = xf {
            *self.primary_consumer.lock().unwrap() = Some(xc);
        }

        // Promote next decode to primary
        self.decode_handle = next_decode;
        self.next_decode_handle = None;

        // Promote the next-track duration limit so subsequent set_trim calls
        // bound the promoted decode thread.
        self.effective_duration_secs = std::mem::replace(
            &mut self.next_effective_duration_secs,
            Arc::new(AtomicF32::new(f32::INFINITY)),
        );

        // Reset crossfade state
        self.xfade_active.store(false, Ordering::SeqCst);
        self.xfade_out_gain.store(1.0, Ordering::SeqCst);
        self.xfade_in_gain.store(0.0, Ordering::SeqCst);
        self.xfade_start.lock().unwrap().take();
        self.xfade_micro_frame.store(0, Ordering::SeqCst);
        self.xfade_duration_ms.store(0, Ordering::SeqCst);

        // Reset position tracking for the new track,
        // accounting for the seconds already consumed during crossfade
        let ts = self.trim_start.load(Ordering::SeqCst);
        self.paused_at = ts + xf_elapsed;
        self.pause_fade_started = None;
        self.start = Some(Instant::now());

        path
    }

    /// Immediately cancel a pending crossfade.
    ///
    /// Stops the next-track decode thread, removes its consumer, clears the
    /// next path, and resets gains back to identity (out = 1.0, in = 0.0).
    fn crossfade_abort(&mut self) {
        self.xfade_active.store(false, Ordering::SeqCst);
        if let Some(h) = self.next_decode_handle.take() {
            self.next_alive.store(false, Ordering::SeqCst);
            let _ = h.join();
        }
        self.xfade_consumer.lock().unwrap().take();
        self.next_path = None;
        self.xfade_out_gain.store(1.0, Ordering::SeqCst);
        self.xfade_in_gain.store(0.0, Ordering::SeqCst);
    }

    /// The path of the currently prepared (next) track, if any.
    fn next_path(&self) -> Option<PathBuf> {
        self.next_path.clone()
    }
}
