use std::{
    fs::File,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::{self, sleep},
    time::{Duration, Instant},
};

use audioadapter_buffers::number_to_float::InterleavedNumbers;
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

use cpal::{
    Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use super::{AudioBackend, viz_source::SharedSamples};
use crate::{
    Track,
    config::load_config,
    dsp::{db_meter::DbMeter, mini_eq::TripleBandEq, xpander::Expander},
};

use ringbuf::RingBuffer;

pub struct SymphoniaBackend {
    samples: SharedSamples,
    playing: Arc<AtomicBool>,
    start: Option<Instant>,
    paused_at: f32,
    volume: Arc<AtomicU32>,
    stream: Option<Stream>,
    alive: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    decode_handle: Option<std::thread::JoinHandle<()>>,
    low_gain: Arc<AtomicU32>,
    mid_gain: Arc<AtomicU32>,
    high_gain: Arc<AtomicU32>,
    expander_width: Arc<AtomicU32>,
    db_meter_l: Arc<Mutex<DbMeter>>,
    db_meter_r: Arc<Mutex<DbMeter>>,
    fade_state: Arc<AtomicU32>,  // 0=none, 1=fade_in, 2=fade_out
    fade_start: Arc<Mutex<Option<Instant>>>,
    fade_duration_ms: u32,  // milliseconds
    paused_during_fade: Arc<AtomicBool>,  // true if paused and waiting for fade to finish
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
            alive: Arc::new(AtomicBool::new(true)),
            finished: Arc::new(AtomicBool::new(false)),
            decode_handle: None,
            low_gain: Arc::new(AtomicU32::new(100)),
            mid_gain: Arc::new(AtomicU32::new(100)),
            high_gain: Arc::new(AtomicU32::new(100)),
            expander_width: Arc::new(AtomicU32::new(100)),
            db_meter_l: Arc::new(Mutex::new(DbMeter::new())),
            db_meter_r: Arc::new(Mutex::new(DbMeter::new())),
            fade_state: Arc::new(AtomicU32::new(0)),
            fade_start: Arc::new(Mutex::new(None)),
            fade_duration_ms: 350,
            paused_during_fade: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_player(&mut self, path: &Path, seek: f32) {
        self.alive.store(true, Ordering::SeqCst);
        self.finished.store(false, Ordering::SeqCst);
        // const MIN_BUFFER: usize = 4096;
        let samples_viz = self.samples.clone();
        let playing = self.playing.clone();
        let volume = self.volume.clone();

        // let audio_buf: AudioBuffer = Arc::new(Mutex::new(Vec::with_capacity(48_000)));

        playing.store(true, Ordering::SeqCst);
        self.start = Some(Instant::now());

        // ================= CPAL INIT =================
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let mut config: cpal::StreamConfig = device.default_output_config().unwrap().into();

        let output_sr = config.sample_rate as usize;
        let ring = RingBuffer::<f32>::new(output_sr * 4);
        let (mut producer, mut consumer) = ring.split();
        config.channels = 2; // estéreo

        // ================= DECODE THREAD =================
        let finished = self.finished.clone();
        let path = path.to_owned();
        let pl = playing.clone();
        let alive_cl = self.alive.clone();

        let handle = thread::spawn(move || {
            let file = match File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[SymphoniaBackend] failed to open '{}': {}", path.display(), e);
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
                    eprintln!("[SymphoniaBackend] format probe failed for '{}': {:?}", path.display(), e);
                    finished.store(true, Ordering::SeqCst);
                    return;
                }
            };

            let mut format = probed.format;
            let track = match format.default_track() {
                Some(t) => t,
                None => {
                    eprintln!("[SymphoniaBackend] no default track found for '{}'", path.display());
                    finished.store(true, Ordering::SeqCst);
                    return;
                }
            };

            let channels = match track.codec_params.channels {
                Some(c) => c.count(),
                None => {
                    eprintln!("[SymphoniaBackend] missing channel info for '{}'", path.display());
                    finished.store(true, Ordering::SeqCst);
                    return;
                }
            };

            let input_sr = match track.codec_params.sample_rate {
                Some(sr) => sr as usize,
                None => {
                    eprintln!("[SymphoniaBackend] missing sample rate for '{}'", path.display());
                    finished.store(true, Ordering::SeqCst);
                    return;
                }
            };

            let mut params = track.codec_params.clone();
            params.sample_rate = Some(output_sr as u32);

            let mut decoder = match symphonia::default::get_codecs().make(&params, &DecoderOptions::default()) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("[SymphoniaBackend] decoder creation failed for '{}': {:?}", path.display(), e);
                    finished.store(true, Ordering::SeqCst);
                    return;
                }
            };

            if seek > 0.0 {
                let _ = format.seek(
                    SeekMode::Accurate,
                    SeekTo::Time {
                        time: seek.into(),
                        track_id: Some(track.id),
                    },
                );
            }

            // ================= RESAMPLING =================
            let chunk_size = 128;
            let mut interleaved = Vec::<f32>::new();
            let mut resampler = Fft::<f32>::new(
                input_sr,
                output_sr,
                chunk_size,
                2,
                channels,
                rubato::FixedSync::Output,
            )
            .unwrap();

            while alive_cl.load(Ordering::SeqCst) {
                if !pl.load(Ordering::SeqCst) {
                    thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }

                let packet = match format.next_packet() {
                    Ok(p) => p,
                    Err(_) => break,
                };

                let decoded = match decoder.decode(&packet) {
                    Ok(decoded) => decoded,
                    Err(_) => continue,
                };
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                buf.copy_interleaved_ref(decoded);

                for frame in buf.samples().chunks(channels) {
                    let (l, r) = if channels == 1 {
                        (frame[0], frame[0])
                    } else {
                        (frame[0], frame[1])
                    };

                    interleaved.push(l);
                    interleaved.push(r);

                    let needed = resampler.input_frames_next();

                    if interleaved.len() >= needed * 2 {
                        let input =
                            InterleavedNumbers::new(&interleaved[..needed * 2], 2, needed).unwrap();

                        let output = resampler.process(&input, 0, None).unwrap();
                        let out = output.take_data();

                        while alive_cl.load(Ordering::SeqCst)
                            && producer.len() + out.len() > producer.capacity()
                        {
                            thread::sleep(Duration::from_millis(1));
                        }
                        if !alive_cl.load(Ordering::SeqCst) {
                            break;
                        }
                        let _ = producer.push_slice(&out);

                        interleaved.drain(..needed * 2);
                    }
                    if interleaved.capacity() > 8192 {
                        interleaved.shrink_to(4096);
                    }
                }
            }
            // If we exited the packet loop, there may be remaining interleaved samples
            // that need to be flushed through the resampler so playback reaches true EOF.
            while alive_cl.load(Ordering::SeqCst) && !interleaved.is_empty() {
                let needed = resampler.input_frames_next();

                if needed == 0 {
                    break;
                }

                // Prepare input buffer, padding with zeros if necessary to satisfy 'needed'.
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

                while alive_cl.load(Ordering::SeqCst)
                    && producer.len() + out.len() > producer.capacity()
                {
                    thread::sleep(Duration::from_millis(1));
                }
                if !alive_cl.load(Ordering::SeqCst) {
                    break;
                }

                if out.is_empty() {
                    interleaved.clear();
                    break;
                }
                let _ = producer.push_slice(&out);

                // Remove the frames we consumed from the interleaved buffer.
                if interleaved.len() >= needed * 2 {
                    interleaved.drain(..needed * 2);
                } else {
                    interleaved.clear();
                }
            }
            sleep(Duration::from_millis(100));
            finished.store(true, Ordering::SeqCst);
        });
        self.decode_handle = Some(handle);
        // ================= CPAL STREAM (output and now DSP processing) =================
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
        let fade_duration_ms = self.fade_duration_ms;
        let paused_during_fade = self.paused_during_fade.clone();

        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    let state = fade_state.load(Ordering::Relaxed);
                    let is_playing = playing.load(Ordering::Relaxed);
                    let is_paused = paused_during_fade.load(Ordering::Relaxed);
                    let has_fade = state != 0;

                    // If not playing/paused and no fade, output silence
                    if !is_playing && !has_fade {
                        for frame in out.chunks_mut(2) {
                            frame.fill(0.0);
                        }
                        return;
                    }

                    // Calculate fade multiplier
                    let mut fade_mult = 1.0f32;
                    if state == 1 || state == 2 {  // fade_in or fade_out
                        if let Ok(Some(start)) = fade_start.lock().map(|f| *f) {
                            let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                            let fade_dur_f32 = fade_duration_ms as f32;
                            
                            if elapsed_ms >= fade_dur_f32 {
                                // Fade completed
                                if state == 1 {  // fade_in complete
                                    fade_mult = 1.0;
                                    fade_state.store(0, Ordering::Relaxed);
                                } else {  // fade_out complete
                                    fade_mult = 0.0;
                                    fade_state.store(0, Ordering::Relaxed);
                                    // Stop playback after fade-out completes
                                    if is_paused {
                                        playing.store(false, Ordering::Relaxed);
                                    }
                                }
                            } else {
                                let progress = elapsed_ms / fade_dur_f32;
                                fade_mult = if state == 1 {
                                    progress  // fade in: 0 -> 1
                                } else {
                                    1.0 - progress  // fade out: 1 -> 0
                                };
                            }
                        }
                    }

                    let mut left_samples = Vec::with_capacity(out.len() / 2);
                    let mut right_samples = Vec::with_capacity(out.len() / 2);
                    let mut viz = samples_viz.lock().unwrap();
                    let vol = volume.load(Ordering::Relaxed) as f32 / 100.0 * fade_mult;
                    let g_l = low_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_m = mid_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_h = high_g.load(Ordering::Relaxed) as f32 / 100.0;
                    expander_stereo.set_width(width.load(Ordering::Relaxed) as f32 / 100.0);

                    // Clear visualizer if fade-out just completed
                    if state == 2 && fade_mult <= 0.001 {
                        viz.clear();
                    }

                    eq_l.update_all(g_l, g_m, g_h, output_sr as f32);
                    eq_r.update_all(g_l, g_m, g_h, output_sr as f32);
                    // Create a temporary vector to hold samples for this block
                    let mut temp_samples = Vec::with_capacity(out.len());

                    for frame in out.chunks_mut(2) {
                        // Read and process if playing or during fade-in/out
                        if is_playing || (state != 0) {
                            if let (Some(s_l), Some(s_r)) = (consumer.pop(), consumer.pop()) {
                                let eq_l_out = eq_l.process(s_l);
                                let eq_r_out = eq_r.process(s_r);

                                let (final_l, final_r) =
                                    expander_stereo.process_stereo_width(eq_l_out, eq_r_out);

                                frame[0] = final_l * vol;
                                frame[1] = final_r * vol;

                                // Push to local vec instead of locking
                                left_samples.push(frame[0]);
                                right_samples.push(frame[1]);
                                
                                // Update visualizer during playback and any fade (samples already have fade applied)
                                // It will be cleared when fade-out completes
                                if is_playing || (state != 0) {
                                    viz.push(frame[0]);
                                    viz.push(frame[1]);
                                }
                                
                                temp_samples.push(frame[0]);
                                temp_samples.push(frame[1]);
                            } else {
                                // No samples available, output silence
                                frame[0] = 0.0;
                                frame[1] = 0.0;
                            }
                        } else {
                            // Not playing and no fade, output silence
                            frame[0] = 0.0;
                            frame[1] = 0.0;
                        }
                    }
                    if let Ok(mut m_l) = db_meter_l.try_lock() {
                        m_l.process_buffer(&left_samples);
                    }
                    if let Ok(mut m_r) = db_meter_r.try_lock() {
                        m_r.process_buffer(&right_samples);
                    }
                },
                |e| eprintln!("audio error: {e}"),
                None,
            )
            .unwrap();

        stream.play().unwrap();
        self.stream = Some(stream);
    }
}

impl AudioBackend for SymphoniaBackend {
    fn load(&mut self, track: &Track) {
        self.stop();
        self.spawn_player(&track.path, 0.0);
    }

    fn play(&mut self) {
        if self.start.is_none() {
            self.start = Some(Instant::now());
        }
        self.playing.store(true, Ordering::SeqCst);
        self.paused_during_fade.store(false, Ordering::SeqCst);  // Clear fade pause state
        self.fade_state.store(1, Ordering::SeqCst);  // fade_in
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    fn pause(&mut self) {
        // Freeze the position immediately at this exact moment
        if let Some(start) = self.start {
            self.paused_at += start.elapsed().as_secs_f32();
            self.start = None;
        }
        // Clear visualization samples
        self.samples.lock().unwrap().clear();
        // Stop playback immediately, fade happens in audio thread
        self.paused_during_fade.store(true, Ordering::SeqCst);
        self.fade_state.store(2, Ordering::SeqCst);  // fade_out
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    fn stop(&mut self) {
        self.playing.store(false, Ordering::SeqCst);
        self.alive.store(false, Ordering::SeqCst);
        self.start = None;
        self.paused_at = 0.0;
        if let Some(h) = self.decode_handle.take() {
            let _ = h.join();
        }
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        self.samples.lock().unwrap().clear();
    }

    fn set_volume(&self, volume: f32) {
        self.volume.store((volume * 100.0) as u32, Ordering::SeqCst);
    }

    fn seek(&mut self, path: &Path, seconds: f32) {
        self.stop();
        self.paused_at = seconds;
        self.spawn_player(path, seconds);
    }

    fn position(&self) -> f32 {
        match self.start {
            Some(t) => self.paused_at + t.elapsed().as_secs_f32(),
            None => self.paused_at,
        }
    }

    fn samples(&self) -> SharedSamples {
        self.samples.clone()
    }

    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    fn low_gain(&self, gain: f32) {
        self.low_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    fn mid_gain(&self, gain: f32) {
        self.mid_gain.store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    fn high_gain(&self, gain: f32) {
        self.high_gain
            .store((gain * 100.0) as u32, Ordering::SeqCst);
    }

    fn set_expander_width(&self, width: f32) {
        self.expander_width
            .store((width * 100.0) as u32, Ordering::SeqCst);
    }

    fn get_db_loudness(&self) -> (f32, f32) {
        (self.db_meter_r.lock().unwrap().current_db, self.db_meter_l.lock().unwrap().current_db)
    }
}
