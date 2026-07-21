use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::{self, sleep},
    time::{Duration, Instant},
};

use audioadapter_buffers::number_to_float::InterleavedNumbers;
use atomic_float::AtomicF32;
use cpal::{
    Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{Consumer, Producer, RingBuffer};
use rubato::{Fft, Resampler};

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

use super::{AudioBackend, crossfade, viz_source::SharedSamples};
use crate::{
    Track,
    config::load_config,
    dsp::{db_meter::DbMeter, mini_eq::TripleBandEq, xpander::Expander},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the ring buffer (in stereo frames) for each decode thread.
/// At 48 kHz this gives ≈ 4 seconds of buffer.
const RING_CAPACITY_FRAMES: usize = 192_000;

// ---------------------------------------------------------------------------
// SymphoniaBackend
// ---------------------------------------------------------------------------

pub struct SymphoniaBackend {
    samples: SharedSamples,
    playing: Arc<AtomicBool>,
    start: Option<Instant>,
    paused_at: f32,
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
    max_output_frames: Arc<AtomicU32>,
}

impl SymphoniaBackend {
    pub fn new(samples: SharedSamples) -> Self {
        Self {
            samples,
            playing: Arc::new(AtomicBool::new(false)),
            start: None,
            paused_at: 0.0,
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
            max_output_frames: Arc::new(AtomicU32::new(u32::MAX)),
        }
    }

    // -----------------------------------------------------------------------
    // Internal: decode loop (primary)
    // -----------------------------------------------------------------------
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
        let max_frames = self.max_output_frames.clone();

        let decode_handle = thread::spawn(move || {
            Self::decode_loop(
                &path, output_sr, producer, &alive_cl, &pl, &finished,
                max_frames, seek_seconds, samples_viz_decode,
            );
        });
        self.decode_handle = Some(decode_handle);

        // ---- CPAL output stream ----
        let mut eq_l = TripleBandEq::new();
        let mut eq_r = TripleBandEq::new();
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
                            let eq_nl = eq_l.process(n_l);
                            let eq_nr = eq_r.process(n_r);
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

    // -----------------------------------------------------------------------
    // Static decode loop (shared by primary and next decode threads)
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn decode_loop(
        path: &Path,
        output_sr: usize,
        mut producer: Producer<f32>,
        alive: &AtomicBool,
        playing: &AtomicBool,
        finished: &AtomicBool,
        max_frames: Arc<AtomicU32>,
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

        let mut decoder = match symphonia::default::get_codecs().make(&params, &DecoderOptions::default()) {
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

        let max_frame_limit = max_frames.load(Ordering::Relaxed);
        let mut frames_pushed: u32 = 0;
        let chunk_size = 128;
        let mut interleaved = Vec::<f32>::new();

        let mut resampler = Fft::<f32>::new(
            input_sr, output_sr, chunk_size, 2, channels,
            rubato::FixedSync::Output,
        )
        .unwrap();

        while alive.load(Ordering::SeqCst) {
            // Stop decoding when we've pushed enough frames (silence trim)
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

                let needed = resampler.input_frames_next();
                if interleaved.len() >= needed * 2 {
                    let input =
                        InterleavedNumbers::new(&interleaved[..needed * 2], 2, needed).unwrap();
                    let output = resampler.process(&input, 0, None).unwrap();
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

                    let n_frames = out.len() / 2;
                    if frames_pushed + n_frames as u32 > max_frame_limit {
                        let allowed = (max_frame_limit - frames_pushed) as usize * 2;
                        if allowed > 0 {
                            let _ = producer.push_slice(&out[..allowed]);
                        }
                        break;
                    }
                    let _ = producer.push_slice(&out);
                    frames_pushed += n_frames as u32;

                    interleaved.drain(..needed * 2);
                }
                if interleaved.capacity() > 8192 {
                    interleaved.shrink_to(4096);
                }
            }
        }

        // Flush remaining samples through resampler
        while alive.load(Ordering::SeqCst) && !interleaved.is_empty() {
            let needed = resampler.input_frames_next();
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
            let output = match resampler.process(&input, 0, None) {
                Ok(o) => o,
                Err(_) => break,
            };
            let out = output.take_data();
            while alive.load(Ordering::SeqCst)
                && producer.len() + out.len() > producer.capacity()
            {
                thread::sleep(Duration::from_millis(1));
            }
            if !alive.load(Ordering::SeqCst) {
                break;
            }
            if !out.is_empty() {
                let n_frames = out.len() / 2;
                if frames_pushed + n_frames as u32 > max_frame_limit {
                    let allowed = (max_frame_limit - frames_pushed) as usize * 2;
                    if allowed > 0 {
                        let _ = producer.push_slice(&out[..allowed]);
                    }
                    break;
                }
                let _ = producer.push_slice(&out);
                frames_pushed += n_frames as u32;
            }
            if interleaved.len() >= needed * 2 {
                interleaved.drain(..needed * 2);
            } else {
                interleaved.clear();
            }
        }

        // Let consumer drain before signalling finished
        sleep(Duration::from_millis(100));
        finished.store(true, Ordering::SeqCst);
    }
}

// ===========================================================================
// AudioBackend trait implementation
// ===========================================================================

impl AudioBackend for SymphoniaBackend {
    fn load(&mut self, track: &Track) {
        self.stop();
        self.crossfade_abort();
        let seek = self.trim_start.load(Ordering::SeqCst);
        self.spawn_player(&track.path, seek);
    }

    fn play(&mut self) {
        if self.start.is_none() {
            self.start = Some(Instant::now());
        }
        self.playing.store(true, Ordering::SeqCst);
        self.paused_during_fade.store(false, Ordering::SeqCst);
        self.fade_state.store(1, Ordering::SeqCst);
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    fn pause(&mut self) {
        if let Some(start) = self.start {
            self.paused_at += start.elapsed().as_secs_f32();
            self.start = None;
        }
        self.samples.lock().unwrap().clear();
        self.paused_during_fade.store(true, Ordering::SeqCst);
        self.fade_state.store(2, Ordering::SeqCst);
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    fn stop(&mut self) {
        self.crossfade_abort();
        self.next_alive.store(false, Ordering::SeqCst);
        self.playing.store(false, Ordering::SeqCst);
        self.alive.store(false, Ordering::SeqCst);
        self.start = None;
        self.paused_at = 0.0;
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

    fn seek(&mut self, path: &Path, seconds: f32) {
        self.crossfade_abort();
        self.stop();
        let ts = self.trim_start.load(Ordering::SeqCst);
        let raw_seek = seconds + ts;
        self.paused_at = raw_seek;
        self.spawn_player(path, raw_seek);
    }

    fn position(&self) -> f32 {
        let ts = self.trim_start.load(Ordering::SeqCst);
        let abs = match self.start {
            Some(t) => self.paused_at + t.elapsed().as_secs_f32(),
            None => self.paused_at,
        };
        (abs - ts).max(0.0)
    }

    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    fn samples(&self) -> SharedSamples {
        self.samples.clone()
    }

    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    fn consumer_depleted(&self) -> bool {
        if !self.finished.load(Ordering::SeqCst) {
            return false;
        }
        let pc = self.primary_consumer.lock().unwrap();
        pc.as_ref().is_none_or(|c| c.is_empty())
    }

    fn get_db_loudness(&self) -> (f32, f32) {
        (
            self.db_meter_r.lock().unwrap().current_db,
            self.db_meter_l.lock().unwrap().current_db,
        )
    }

    fn set_volume(&self, volume: f32) {
        self.volume.store((volume * 100.0) as u32, Ordering::SeqCst);
    }

    fn low_gain(&self, gain: f32) {
        self.low_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    fn mid_gain(&self, gain: f32) {
        self.mid_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    fn high_gain(&self, gain: f32) {
        self.high_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    fn set_expander_width(&self, width: f32) {
        self.expander_width.store((width * 100.0) as u32, Ordering::SeqCst);
    }

    fn set_trim(&mut self, start_secs: f32, end_secs: f32, total_output_frames: u32) {
        self.trim_start.store(start_secs, Ordering::SeqCst);
        self.trim_end.store(end_secs, Ordering::SeqCst);
        self.max_output_frames.store(total_output_frames, Ordering::SeqCst);
    }

    fn trim_start(&self) -> f32 {
        self.trim_start.load(Ordering::SeqCst)
    }

    fn trim_end(&self) -> f32 {
        self.trim_end.load(Ordering::SeqCst)
    }

    // ---- Crossfade primitives ----

    fn prepare_next(&mut self, path: &Path, trim_start: f32) {
        self.crossfade_abort();
        self.next_alive.store(true, Ordering::SeqCst);
        self.next_finished.store(false, Ordering::SeqCst);

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
        let max_frames_dummy = Arc::new(AtomicU32::new(u32::MAX));
        let samples_dummy = Arc::new(Mutex::new(Vec::new()));

        let handle = thread::spawn(move || {
            SymphoniaBackend::decode_loop(
                &path_for_closure,
                output_sr,
                producer,
                &alive,
                &dummy_playing,
                &dummy_finished,
                max_frames_dummy,
                trim_start,
                samples_dummy,
            );
        });

        self.next_decode_handle = Some(handle);
        self.next_path = Some(path_for_store);
    }

    fn start_crossfade(&mut self, duration_ms: u32) {
        self.xfade_out_gain.store(1.0, Ordering::SeqCst);
        self.xfade_in_gain.store(0.0, Ordering::SeqCst);
        self.xfade_duration_ms.store(duration_ms, Ordering::SeqCst);
        *self.xfade_start.lock().unwrap() = Some(Instant::now());
        crossfade::reset_micro_fade(&self.xfade_micro_frame);
        self.xfade_active.store(true, Ordering::SeqCst);
    }

    fn is_crossfade_active(&self) -> bool {
        self.xfade_active.load(Ordering::SeqCst)
    }

    fn is_next_finished(&self) -> bool {
        self.next_finished.load(Ordering::SeqCst)
    }

    fn crossfade_gains(&self) -> (f32, f32) {
        (
            self.xfade_out_gain.load(Ordering::SeqCst),
            self.xfade_in_gain.load(Ordering::SeqCst),
        )
    }

    fn set_crossfade_gains(&self, out: f32, in_: f32) {
        self.xfade_out_gain.store(out, Ordering::SeqCst);
        self.xfade_in_gain.store(in_, Ordering::SeqCst);
    }

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
        self.start = Some(Instant::now());

        path
    }

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

    fn next_path(&self) -> Option<PathBuf> {
        self.next_path.clone()
    }
}
