use std::{
    fs::File,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
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
            let file = File::open(path).unwrap();
            let mss = MediaSourceStream::new(Box::new(file), Default::default());

            let probed = get_probe()
                .format(
                    &Hint::new(),
                    mss,
                    &FormatOptions::default(),
                    &MetadataOptions::default(),
                )
                .unwrap();

            let mut format = probed.format;
            let track = format.default_track().unwrap();

            let channels = track.codec_params.channels.unwrap().count();
            let input_sr = track.codec_params.sample_rate.unwrap() as usize;
            let mut params = track.codec_params.clone();
            params.sample_rate = Some(output_sr as u32);

            let mut decoder = symphonia::default::get_codecs()
                .make(&params, &DecoderOptions::default())
                .unwrap();

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

        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    let mut left_samples = Vec::with_capacity(out.len() / 2);
                    let mut right_samples = Vec::with_capacity(out.len() / 2);
                    let mut viz = samples_viz.lock().unwrap();
                    let vol = volume.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_l = low_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_m = mid_g.load(Ordering::Relaxed) as f32 / 100.0;
                    let g_h = high_g.load(Ordering::Relaxed) as f32 / 100.0;
                    expander_stereo.set_width(width.load(Ordering::Relaxed) as f32 / 100.0);

                    eq_l.update_all(g_l, g_m, g_h, output_sr as f32);
                    eq_r.update_all(g_l, g_m, g_h, output_sr as f32);
                    // Create a temporary vector to hold samples for this block
                    let mut temp_samples = Vec::with_capacity(out.len());

                    for frame in out.chunks_mut(2) {
                        if !playing.load(Ordering::Relaxed) {
                            frame.fill(0.0);
                            continue;
                        }
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
                            viz.push(frame[0]);
                            viz.push(frame[1]);
                            temp_samples.push(frame[0]);
                            temp_samples.push(frame[1]);
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
    }

    fn pause(&mut self) {
        if let Some(start) = self.start {
            self.paused_at += start.elapsed().as_secs_f32();
            self.start = None;
        }
        self.samples.lock().unwrap().clear();
        self.playing.store(false, Ordering::SeqCst);
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
