//! Bit-perfect ALSA audio backend for Linux.
//!
//! [`AlsaBackend`] implements the [`AudioBackend`] trait using Symphonia for
//! decoding and **direct ALSA hardware access** for output. It bypasses the
//! system audio daemons (PulseAudio/PipeWire) and plugins such as `dmix` by
//! opening `hw:` devices exclusively, and it never resamples: the hardware is
//! configured at the file's *native* sample rate and bit depth. When the
//! device does not support the track's native format, [`load`](Self::load)
//! returns an error so the dispatcher can fall back to the CPAL backend.
//!
//! # Output model
//!
//! A dedicated **writer thread** replaces the CPAL output callback: it drains
//! the lock-free ring buffer, applies DSP (EQ, stereo widening, volume,
//! crossfade mixing) in f32, converts the frames to the device's native
//! format, and blocks on `snd_pcm_writei`, which paces playback at the
//! hardware rate.
//!
//! # Bit depth notes
//!
//! The f32 DSP pipeline is the inverse of Symphonia's decode conversion, so
//! with neutral DSP settings the output is bit-exact for S16, S24, U8 and F32
//! sources. S32 and F64 sources are decoded to f32 and may differ by a few
//! LSBs (the f32 container has 24-bit mantissa). Lossy codecs (MP3, OGG, …)
//! decode to f32 and are output as `FLOAT_LE` when supported, otherwise they
//! are down-converted.

use std::{
    ffi::CString,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use alsa::{
    Direction, ValueOr,
    card::Card,
    device_name::HintIter,
    pcm::{Access, Format as AlsaFormat, HwParams, PCM, State},
};
use atomic_float::AtomicF32;
use ringbuf::{Consumer, RingBuffer};

use super::{
    AudioBackend, BackendError, crossfade,
    decode::{self, DecodedBufferKind, FileAudio, RING_CAPACITY_FRAMES},
    viz_source::SharedSamples,
};
use crate::{
    Track,
    config::load_config,
    dsp::{db_meter::DbMeter, mini_eq::TripleBandEq, xpander::Expander},
};

/// Period size in frames: the granularity of each hardware write.
/// At 44.1 kHz this is ≈ 5.8 ms.
const PERIOD_FRAMES: usize = 256;
/// Target ring-buffer size of the hardware device in frames.
const BUFFER_FRAMES: usize = 4096;

/// Native output container negotiated with the ALSA device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeFormat {
    /// 16-bit signed integer (`SND_PCM_FORMAT_S16_LE`).
    S16,
    /// 24-bit signed integer in a 32-bit container, LSB justified
    /// (`SND_PCM_FORMAT_S24_LE`).
    S24,
    /// 24-bit signed integer packed in 3 bytes (`SND_PCM_FORMAT_S24_3LE`).
    S243,
    /// 32-bit signed integer (`SND_PCM_FORMAT_S32_LE`).
    S32,
    /// 8-bit unsigned integer (`SND_PCM_FORMAT_U8`).
    U8,
    /// 32-bit float (`SND_PCM_FORMAT_FLOAT_LE`).
    F32,
    /// 64-bit float (`SND_PCM_FORMAT_FLOAT64_LE`).
    F64,
}

impl NativeFormat {
    fn to_alsa(self) -> AlsaFormat {
        match self {
            NativeFormat::S16 => AlsaFormat::s16(),
            NativeFormat::S24 => AlsaFormat::s24(),
            NativeFormat::S243 => AlsaFormat::s24_3(),
            NativeFormat::S32 => AlsaFormat::s32(),
            NativeFormat::U8 => AlsaFormat::U8,
            NativeFormat::F32 => AlsaFormat::float(),
            NativeFormat::F64 => AlsaFormat::float64(),
        }
    }

    /// Bytes per stereo frame in this container.
    fn frame_bytes(self) -> usize {
        match self {
            NativeFormat::S16 => 4,
            NativeFormat::S24 => 8,
            NativeFormat::S243 => 6,
            NativeFormat::S32 => 8,
            NativeFormat::U8 => 2,
            NativeFormat::F32 => 8,
            NativeFormat::F64 => 16,
        }
    }
}

/// f32 → integer scaling that inverts Symphonia's decode conversion.
///
/// The *container* scale depends on the decoded buffer kind; `S32`/`F32`
/// containers are left-justified (value `<< (32 - bits)`) so the final
/// extraction shifts right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Converter {
    /// Scale `2^15`, right-justified.
    S16,
    /// Scale `2^23`, right-justified.
    S24,
    /// Scale `2^31`, left-justified.
    S32,
    /// Scale `2^7` (unsigned).
    U8,
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
}

impl Converter {
    fn for_kind(kind: DecodedBufferKind) -> Self {
        match kind {
            DecodedBufferKind::S16 => Converter::S16,
            DecodedBufferKind::S24 => Converter::S24,
            DecodedBufferKind::S32 => Converter::S32,
            DecodedBufferKind::U8 => Converter::U8,
            DecodedBufferKind::F32 => Converter::F32,
            DecodedBufferKind::F64 => Converter::F64,
        }
    }

    /// Convert one f32 sample to the integer value for an output with
    /// `out_bits` significant bits (16, 24 or 32).
    fn int_value(self, f: f32, out_bits: u32) -> i32 {
        let raw: i32 = match self {
            Converter::S16 => (f * 32_768.0).round() as i32,
            Converter::S24 => (f * 8_388_608.0).round() as i32,
            Converter::S32 | Converter::F32 => (f as f64 * 2_147_483_648.0).round() as i32,
            Converter::U8 => ((f + 1.0) * 128.0).round() as i32,
            Converter::F64 => (f as f64 * 2_147_483_648.0).round() as i32,
        };
        if matches!(self, Converter::S32 | Converter::F32 | Converter::F64) {
            raw >> (32 - out_bits)
        } else {
            raw
        }
    }

    /// Append one sample to `out` in the given native container.
    fn pack(self, out: &mut Vec<u8>, f: f32, fmt: NativeFormat) {
        match fmt {
            NativeFormat::F32 => out.extend_from_slice(&f.to_le_bytes()),
            NativeFormat::F64 => out.extend_from_slice(&(f as f64).to_le_bytes()),
            NativeFormat::S16 => {
                let v = self.int_value(f, 16) as i16;
                out.extend_from_slice(&v.to_le_bytes());
            }
            NativeFormat::S24 => {
                let v = self.int_value(f, 24);
                out.extend_from_slice(&v.to_le_bytes());
            }
            NativeFormat::S243 => {
                let v = self.int_value(f, 24);
                out.extend_from_slice(&v.to_le_bytes()[..3]);
            }
            NativeFormat::S32 => {
                let v = self.int_value(f, 32);
                out.extend_from_slice(&v.to_le_bytes());
            }
            NativeFormat::U8 => out.push(self.int_value(f, 8) as u8),
        }
    }
}

/// Output format candidates for a file, in preference order.
///
/// Integer sources are only offered their exact bit depth (down-converting
/// would break bit-perfection). Lossy f32 sources may fall back to any
/// container the device supports.
fn output_candidates(kind: DecodedBufferKind, bits: u32) -> &'static [NativeFormat] {
    match kind {
        DecodedBufferKind::S16 => &[NativeFormat::S16],
        DecodedBufferKind::S24 => &[NativeFormat::S24, NativeFormat::S243],
        DecodedBufferKind::S32 if bits <= 16 => &[NativeFormat::S16],
        DecodedBufferKind::S32 if bits <= 24 => &[NativeFormat::S24, NativeFormat::S243],
        DecodedBufferKind::S32 => &[NativeFormat::S32],
        DecodedBufferKind::U8 => &[NativeFormat::U8],
        DecodedBufferKind::F32 => &[
            NativeFormat::F32,
            NativeFormat::S32,
            NativeFormat::S24,
            NativeFormat::S243,
            NativeFormat::S16,
        ],
        DecodedBufferKind::F64 => &[NativeFormat::F64, NativeFormat::F32, NativeFormat::S32],
    }
}

// ---------------------------------------------------------------------------
// AlsaBackend
// ---------------------------------------------------------------------------

/// Audio backend using Symphonia (decode) + direct ALSA (bit-perfect output).
///
/// ## Fields
///
/// Same shape as [`SymphoniaBackend`](super::symphonia_backend::SymphoniaBackend),
/// except the CPAL stream is replaced by `writer_handle` (the thread owning
/// the open [`PCM`]) and the output sample rate follows the *file* rather
/// than the device.
pub struct AlsaBackend {
    samples: SharedSamples,
    playing: Arc<AtomicBool>,
    start: Option<Instant>,
    paused_at: f32,
    pause_fade_started: Option<Instant>,
    volume: Arc<AtomicU32>,
    device_name: String,
    writer_handle: Option<thread::JoinHandle<()>>,
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
    current_native_format: Option<NativeFormat>,

    // Crossfade
    xfade_active: Arc<AtomicBool>,
    xfade_out_gain: Arc<AtomicF32>,
    xfade_in_gain: Arc<AtomicF32>,
    xfade_duration_ms: Arc<AtomicU32>,
    xfade_start: Arc<Mutex<Option<Instant>>>,
    xfade_micro_frame: Arc<AtomicU32>,

    // Shared consumers for the writer thread (hot-swap via Arc<Mutex<Option<...>>>)
    primary_consumer: Arc<Mutex<Option<Consumer<f32>>>>,
    xfade_consumer: Arc<Mutex<Option<Consumer<f32>>>>,

    // Silence trim
    trim_start: Arc<AtomicF32>,
    trim_end: Arc<AtomicF32>,
    effective_duration_secs: Arc<AtomicF32>,
    next_effective_duration_secs: Arc<AtomicF32>,
}

impl AlsaBackend {
    /// Construct a new [`AlsaBackend`] bound to `device_name` (an `hw:`
    /// device such as `hw:0,0`).
    pub fn new(samples: SharedSamples, device_name: String) -> Self {
        Self {
            samples,
            playing: Arc::new(AtomicBool::new(false)),
            start: None,
            paused_at: 0.0,
            pause_fade_started: None,
            volume: Arc::new(AtomicU32::new((load_config().volume * 100.0) as u32)),
            device_name,
            writer_handle: None,
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
            sample_rate: 44100.0,
            current_native_format: None,
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

    /// Resolve the ALSA device name to use for bit-perfect output.
    ///
    /// * An explicit `configured` name is used as-is if it opens, or resolved
    ///   as an ALSA card name (`hw:<index>,0`).
    /// * Otherwise the first openable `hw:` playback device is auto-detected.
    ///
    /// Returns an error when no usable device exists (e.g. the device is busy
    /// or there is no hardware), signalling the caller to fall back to CPAL.
    pub fn resolve_device(configured: &str) -> Result<String, String> {
        let configured = configured.trim();
        if !configured.is_empty() {
            if Self::can_open(configured) {
                return Ok(configured.to_string());
            }
            if let Ok(name) = CString::new(configured)
                && let Ok(card) = Card::from_str(&name)
            {
                let dev = format!("hw:{},0", card.get_index());
                if Self::can_open(&dev) {
                    return Ok(dev);
                }
            }
            return Err(format!("cannot open configured ALSA device '{configured}'"));
        }

        if let Ok(hints) = HintIter::new_str(None, "pcm") {
            for hint in hints {
                if let Some(name) = hint.name
                    && name.starts_with("hw:")
                    && Self::can_open(&name)
                {
                    return Ok(name);
                }
            }
        }

        // Fallback: scan raw card/device indices — covers devices not
        // enumerated by the hint interface (e.g. HDMI).
        for card_idx in 0..32 {
            for dev_idx in 0..32 {
                let dev = format!("hw:{card_idx},{dev_idx}");
                if Self::can_open(&dev) {
                    return Ok(dev);
                }
            }
        }
        Err("no usable hw: playback device found".to_string())
    }

    /// Whether the given device can be opened for playback right now.
    fn can_open(name: &str) -> bool {
        PCM::new(name, Direction::Playback, false).is_ok()
    }

    /// Enumerate usable `hw:` playback devices as `(display, value)` pairs.
    ///
    /// The display label includes the card name when resolvable; `value` is
    /// the ALSA device name to store in the configuration. Returns an empty
    /// list when no playback device is available.
    pub fn list_devices() -> Vec<(String, String)> {
        let mut devices: Vec<(String, String)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();

        if let Ok(hints) = HintIter::new_str(None, "pcm") {
            for hint in hints {
                if let Some(name) = hint.name
                    && name.starts_with("hw:")
                    && Self::can_open(&name)
                    && !seen.contains(&name)
                {
                    seen.push(name.clone());
                    devices.push((Self::describe_device(&name), name));
                }
            }
        }

        // Fallback: scan raw card/device indices — covers devices not
        // enumerated by the hint interface (e.g. HDMI).
        for card_idx in 0..32 {
            for dev_idx in 0..32 {
                let dev = format!("hw:{card_idx},{dev_idx}");
                if Self::can_open(&dev) && !seen.contains(&dev) {
                    seen.push(dev.clone());
                    devices.push((Self::describe_device(&dev), dev));
                }
            }
        }
        devices
    }

    /// Human-readable label for a `hw:C,D` device (e.g. `hw:0,3 · HD-Audio`).
    fn describe_device(dev: &str) -> String {
        if let Some((card_part, _)) = dev.split_once(',') {
            if let Ok(name) = CString::new(card_part)
                && let Ok(card) = Card::from_str(&name)
            {
                return format!("{dev} · {}", card.get_name().unwrap_or_default());
            }
        }
        dev.to_string()
    }

    /// Open the hardware device configured for the file's native rate and the
    /// first supported output container.
    pub fn open_pcm(&self, file: &FileAudio) -> Result<(PCM, NativeFormat), BackendError> {
        let candidates = output_candidates(file.kind, file.bits_per_sample);
        let mut last_err: Option<String> = None;
        for fmt in candidates {
            match Self::try_open(&self.device_name, file.sample_rate, *fmt) {
                Ok(pcm) => return Ok((pcm, *fmt)),
                Err(e) => last_err = Some(e),
            }
        }
        Err(BackendError(format!(
            "{}: no compatible format for {} Hz (last error: {})",
            self.device_name,
            file.sample_rate,
            last_err.unwrap_or_default()
        )))
    }

    /// Open and configure a PCM at exactly `rate` with `fmt`.
    ///
    /// The negotiated rate and format are verified afterwards: any deviation
    /// (which would imply hardware resampling or conversion) is rejected.
    fn try_open(device: &str, rate: usize, fmt: NativeFormat) -> Result<PCM, String> {
        let pcm = PCM::new(device, Direction::Playback, false)
            .map_err(|e| format!("open '{device}': {e}"))?;

        let hwp = HwParams::any(&pcm).map_err(|e| format!("hw params: {e}"))?;
        hwp.set_channels(2).map_err(|e| format!("channels: {e}"))?;
        hwp.set_rate(rate as u32, ValueOr::Nearest).map_err(|e| format!("rate: {e}"))?;
        hwp.set_format(fmt.to_alsa()).map_err(|e| format!("format {:?}: {e}", fmt.to_alsa()))?;
        hwp.set_access(Access::RWInterleaved).map_err(|e| format!("access: {e}"))?;
        hwp.set_period_size_near(PERIOD_FRAMES as i64, ValueOr::Nearest)
            .map_err(|e| format!("period: {e}"))?;
        hwp.set_buffer_size_near(BUFFER_FRAMES as i64)
            .map_err(|e| format!("buffer: {e}"))?;
        pcm.hw_params(&hwp).map_err(|e| format!("set params: {e}"))?;
        drop(hwp);

        // Bit-perfect guard: the hardware must accept the exact rate/format.
        let cur = pcm.hw_params_current().map_err(|e| format!("read params: {e}"))?;
        if cur.get_rate().map_err(|e| format!("read rate: {e}"))? != rate as u32 {
            return Err(format!("rate {rate} Hz not supported"));
        }
        if cur.get_format().map_err(|e| format!("read format: {e}"))? != fmt.to_alsa() {
            return Err(format!("format {:?} not supported", fmt.to_alsa()));
        }

        // Low-latency soft parameters: start once a period is available.
        let period = cur.get_period_size().map_err(|e| format!("period size: {e}"))?;
        let swp = pcm.sw_params_current().map_err(|e| format!("sw params: {e}"))?;
        swp.set_start_threshold(period).map_err(|e| format!("start threshold: {e}"))?;
        swp.set_avail_min(period).map_err(|e| format!("avail min: {e}"))?;
        pcm.sw_params(&swp).map_err(|e| format!("apply sw params: {e}"))?;

        drop(swp);
        drop(cur);
        Ok(pcm)
    }

    // -----------------------------------------------------------------------
    // Internal: spawn decode thread + ALSA writer thread
    // -----------------------------------------------------------------------
    //
    // Starts a Symphonia decode thread feeding `primary_consumer`, then a
    // writer thread owning the PCM that drains the consumer, applies DSP, and
    // blocks on snd_pcm_writei.
    //
    // `seek_seconds` — seek the decoder to this absolute position before playback.
    fn spawn_player(&mut self, path: &Path, seek_seconds: f32) -> Result<(), BackendError> {
        // The file's native properties drive the hardware configuration.
        let file_audio = decode::probe_audio(path).map_err(BackendError)?;
        if file_audio.channels == 0 {
            return Err(BackendError("no audio channels".to_string()));
        }
        let output_sr = file_audio.sample_rate;
        self.sample_rate = output_sr as f32;

        // Open the hardware at the native rate before spawning anything.
        let (pcm, native_fmt) = self.open_pcm(&file_audio)?;
        self.current_native_format = Some(native_fmt);
        let converter = Converter::for_kind(file_audio.kind);

        self.alive.store(true, Ordering::SeqCst);
        self.finished.store(false, Ordering::SeqCst);

        let samples_viz = self.samples.clone();
        let samples_viz_decode = samples_viz.clone();
        let playing = self.playing.clone();
        let volume = self.volume.clone();

        playing.store(true, Ordering::SeqCst);
        self.start = Some(Instant::now());

        let ring = RingBuffer::<f32>::new(RING_CAPACITY_FRAMES * 2);
        let (producer, consumer) = ring.split();
        *self.primary_consumer.lock().unwrap() = Some(consumer);

        let finished = self.finished.clone();
        let path = path.to_owned();
        let pl = playing.clone();
        let alive_cl = self.alive.clone();
        let effective_dur = self.effective_duration_secs.clone();

        let decode_handle = thread::spawn(move || {
            decode::decode_loop(
                &path, output_sr, producer, &alive_cl, &pl, &finished,
                &effective_dur, seek_seconds, samples_viz_decode,
            );
        });
        self.decode_handle = Some(decode_handle);

        // ---- ALSA writer thread ----
        let primary_consumer = self.primary_consumer.clone();
        let xfade_consumer = self.xfade_consumer.clone();
        let xfade_active = self.xfade_active.clone();
        let xfade_out = self.xfade_out_gain.clone();
        let xfade_in = self.xfade_in_gain.clone();
        let xfade_micro = self.xfade_micro_frame.clone();
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
        let alive = self.alive.clone();

        let writer_handle = thread::spawn(move || {
            Self::writer_loop(
                pcm,
                native_fmt,
                converter,
                PERIOD_FRAMES,
                sample_rate_f(output_sr),
                alive,
                playing,
                primary_consumer,
                xfade_consumer,
                xfade_active,
                xfade_out,
                xfade_in,
                xfade_micro,
                low_g,
                mid_g,
                high_g,
                width,
                volume,
                db_meter_l,
                db_meter_r,
                fade_state,
                fade_start,
                fade_duration_ms,
                paused_during_fade,
                samples_viz,
            );
        });
        self.writer_handle = Some(writer_handle);

        Ok(())
    }

    /// The writer thread: render DSP-mixed audio and block on the hardware.
    ///
    /// Mirrors the CPAL output callback (fades, crossfade mixing, EQ,
    /// expander, volume, dB meters, visualisation) but driven by a blocking
    /// `writei` loop instead of a device callback.
    #[allow(clippy::too_many_arguments)]
    fn writer_loop(
        pcm: PCM,
        fmt: NativeFormat,
        converter: Converter,
        period_frames: usize,
        sample_rate_f: f32,
        alive: Arc<AtomicBool>,
        playing: Arc<AtomicBool>,
        primary_consumer: Arc<Mutex<Option<Consumer<f32>>>>,
        xfade_consumer: Arc<Mutex<Option<Consumer<f32>>>>,
        xfade_active: Arc<AtomicBool>,
        xfade_out: Arc<AtomicF32>,
        xfade_in: Arc<AtomicF32>,
        xfade_micro: Arc<AtomicU32>,
        low_g: Arc<AtomicU32>,
        mid_g: Arc<AtomicU32>,
        high_g: Arc<AtomicU32>,
        width: Arc<AtomicU32>,
        volume: Arc<AtomicU32>,
        db_meter_l: Arc<Mutex<DbMeter>>,
        db_meter_r: Arc<Mutex<DbMeter>>,
        fade_state: Arc<AtomicU32>,
        fade_start: Arc<Mutex<Option<Instant>>>,
        fade_duration_ms: Arc<AtomicU32>,
        paused_during_fade: Arc<AtomicBool>,
        samples_viz: SharedSamples,
    ) {
        let mut eq_l = TripleBandEq::new();
        let mut eq_r = TripleBandEq::new();
        let mut eq_xfade_l = TripleBandEq::new();
        let mut eq_xfade_r = TripleBandEq::new();
        let mut expander_stereo = Expander::new();

        let micro_fade_total = crossfade::micro_fade_frame_count(sample_rate_f);

        let io = pcm.io_bytes();
        let frame_bytes = fmt.frame_bytes();
        let mut fbuf = vec![0.0f32; period_frames * 2];
        let mut native = Vec::<u8>::with_capacity(period_frames * 2 * frame_bytes);

        while alive.load(Ordering::SeqCst) {
            let state = fade_state.load(Ordering::Relaxed);
            let is_playing = playing.load(Ordering::Relaxed);
            let is_paused = paused_during_fade.load(Ordering::Relaxed);
            let has_fade = state != 0;
            let xfade = xfade_active.load(Ordering::Relaxed);

            if !is_playing && !has_fade && !xfade {
                fbuf.fill(0.0);
            } else {
                // ---- Play/pause fade ----
                let mut fade_mult = 1.0f32;
                if (state == 1 || state == 2) && let Ok(Some(start)) = fade_start.lock().map(|f| *f)
                {
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

                let mut left_samples = Vec::with_capacity(period_frames);
                let mut right_samples = Vec::with_capacity(period_frames);
                let mut viz = samples_viz.lock().unwrap();

                // Guard: clear viz on pause completion
                if state == 2 && fade_mult <= 0.001 {
                    viz.clear();
                }

                for frame in fbuf.chunks_exact_mut(2) {
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
                        let (fn_l, fn_r) = expander_stereo.process_stereo_width(eq_nl, eq_nr);

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
            }

            // ---- Convert f32 frames to the native container ----
            native.clear();
            for &f in fbuf.iter() {
                converter.pack(&mut native, f, fmt);
            }

            // ---- Block until the hardware accepts the period ----
            let mut written = 0usize;
            let mut errors = 0usize;
            while written < native.len() && alive.load(Ordering::SeqCst) {
                match io.writei(&native[written..]) {
                    Ok(frames) => {
                        if frames == 0 {
                            thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                        written += frames * frame_bytes;
                        errors = 0;
                    }
                    Err(e) => {
                        // Recover from underruns/overruns; bail after too many
                        // consecutive failures (device gone or busy).
                        if pcm.state() == State::XRun {
                            let _ = pcm.prepare();
                        } else {
                            let _ = pcm.recover(e.errno(), true);
                        }
                        errors += 1;
                        if errors > 100 {
                            eprintln!("[ALSA] persistent write error: {e}");
                            return;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                }
            }
        }
    }
}

/// Output sample rate as f32 (used to update the EQ coefficients).
fn sample_rate_f(rate: usize) -> f32 {
    rate as f32
}

// ===========================================================================
// AudioBackend trait implementation
// ===========================================================================

impl AudioBackend for AlsaBackend {
    /// Load a track: stop current playback, abort any crossfade, and spawn a
    /// new decode thread plus ALSA writer thread at the file's native rate.
    ///
    /// Returns an error (and spawns nothing) when the hardware does not
    /// support the track's native rate/format, so the caller can fall back.
    fn load(&mut self, track: &Track) -> Result<(), BackendError> {
        self.stop();
        self.crossfade_abort();
        let seek = self.trim_start.load(Ordering::SeqCst);
        self.spawn_player(&track.path, seek)
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
    /// (state = 2); once complete the playing flag is cleared by the writer.
    /// Visualisation samples are cleared immediately.
    ///
    /// If a crossfade was active, its mixing flag is cleared immediately so
    /// the writer outputs only the primary track's fade-out. The next-track
    /// ring buffer is preserved (the decode thread sleeps when `playing`
    /// becomes false after the fade), so [`play()`](Self::play) can resume
    /// the crossfade from the frozen gains.
    fn pause(&mut self) {
        if let Some(start) = self.start {
            self.paused_at += start.elapsed().as_secs_f32();
            self.start = None;
        }
        if !self.paused_during_fade.load(Ordering::SeqCst) {
            self.pause_fade_started = Some(Instant::now());
        }
        self.samples.lock().unwrap().clear();
        self.xfade_active.store(false, Ordering::SeqCst);
        self.xfade_out_gain.store(0.0, Ordering::Relaxed);
        self.xfade_in_gain.store(0.0, Ordering::Relaxed);
        self.xfade_micro_frame.store(0, Ordering::SeqCst);
        self.paused_during_fade.store(true, Ordering::SeqCst);
        self.fade_state.store(2, Ordering::SeqCst);
        *self.fade_start.lock().unwrap() = Some(Instant::now());
    }

    /// Fully stop playback: abort crossfade, kill decode and writer threads,
    /// release the ALSA device, clear visualisation samples, reset position.
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
        if let Some(h) = self.writer_handle.take() {
            let _ = h.join();
        }
        if let Some(h) = self.decode_handle.take() {
            let _ = h.join();
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
        if let Err(e) = self.spawn_player(path, raw_seek) {
            eprintln!("[ALSA] seek failed: {e}");
        }
    }

    /// Current playback position in seconds (trim-relative).
    fn position(&self) -> f32 {
        let ts = self.trim_start.load(Ordering::SeqCst);
        let abs = match self.start {
            Some(t) => self.paused_at + t.elapsed().as_secs_f32(),
            None => match self.pause_fade_started {
                Some(fade_start) => {
                    let fade_dur = self.fade_duration_ms.load(Ordering::Relaxed) as f32 / 1000.0;
                    self.paused_at + fade_start.elapsed().as_secs_f32().min(fade_dur)
                }
                None => self.paused_at,
            },
        };
        (abs - ts).max(0.0)
    }

    /// Whether audio is currently being output.
    fn is_audible(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    /// The current track's native sample rate (the hardware runs at this rate).
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Shared buffer filled by the writer thread with interleaved stereo frames.
    fn samples(&self) -> SharedSamples {
        self.samples.clone()
    }

    /// Whether the primary decode thread has reached end-of-file.
    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    /// Whether the primary consumer is empty *after* the decoder has finished.
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
    ///   scaled by the native output rate to bound the frame count
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
    /// Returns `false` when the next track's native rate or output format
    /// differs from the current track's — a crossfade between two bit-perfect
    /// streams with different rates would require resampling one of them, so
    /// it is refused and the transition falls back to a normal track change.
    fn prepare_next(&mut self, path: &Path, trim_start: f32) -> bool {
        // Compatibility check against the current hardware configuration.
        let compatible = match decode::probe_audio(path) {
            Ok(fa) => {
                fa.sample_rate == self.sample_rate as usize
                    && self.current_native_format.is_some_and(|fmt| {
                        output_candidates(fa.kind, fa.bits_per_sample).contains(&fmt)
                    })
            }
            Err(_) => false,
        };
        if !compatible {
            eprintln!(
                "[ALSA] skipping crossfade: next track is not bit-perfect compatible \
                 (different native rate or format)"
            );
            return false;
        }

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

    /// Begin the crossfade in the writer thread.
    fn start_crossfade(&mut self, duration_ms: u32) {
        self.xfade_out_gain.store(1.0, Ordering::SeqCst);
        self.xfade_in_gain.store(0.0, Ordering::SeqCst);
        self.xfade_duration_ms.store(duration_ms, Ordering::SeqCst);
        *self.xfade_start.lock().unwrap() = Some(Instant::now());
        crossfade::reset_micro_fade(&self.xfade_micro_frame);
        self.xfade_active.store(true, Ordering::SeqCst);
    }

    /// Whether the writer thread is currently crossfade-mixing two tracks.
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

    /// Re-activate crossfade mixing in the writer after a pause.
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

    /// This backend performs native bit-perfect output.
    fn supports_bit_perfect(&self) -> bool {
        true
    }
}
