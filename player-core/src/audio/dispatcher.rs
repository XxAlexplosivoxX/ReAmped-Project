//! Backend selection and automatic fallback.
//!
//! [`BackendDispatcher`] implements the [`AudioBackend`] trait by delegating
//! to either the direct-ALSA bit-perfect backend or the CPAL backend, and
//! transparently switches to CPAL when bit-perfect mode is disabled, no
//! suitable ALSA hardware device is available, or a load fails (unsupported
//! format/rate, device busy, …).

use std::path::{Path, PathBuf};

use atomic_float::AtomicF32;

use super::{
    AudioBackend, BackendError,
    symphonia_backend::SymphoniaBackend,
    viz_source::SharedSamples,
};
use crate::{Track, config::load_config};

/// Routing backend that dispatches to CPAL or bit-perfect ALSA.
///
/// The concrete backend is chosen once at construction time from the
/// configuration: if `bit_perfect_enabled` is set and a usable ALSA `hw:`
/// device exists, [`AlsaBackend`](crate::audio::alsa_backend::AlsaBackend)
/// is used; otherwise playback goes through the CPAL
/// [`SymphoniaBackend`]. If a bit-perfect [`load`](Self::load) later fails
/// (e.g. the hardware does not support the track's native format), the
/// dispatcher permanently falls back to CPAL and re-applies the latest DSP,
/// volume, and trim parameters so playback continues seamlessly.
pub struct BackendDispatcher {
    samples: SharedSamples,
    inner: Box<dyn AudioBackend>,

    // Last-known parameters, re-applied if a fallback switch occurs.
    trim_start: AtomicF32,
    trim_end: AtomicF32,
    effective_duration: AtomicF32,
    volume: AtomicF32,
    low_gain: AtomicF32,
    mid_gain: AtomicF32,
    high_gain: AtomicF32,
    width: AtomicF32,
}

impl BackendDispatcher {
    /// Construct a dispatcher, selecting the backend from the current config.
    pub fn new(samples: SharedSamples) -> Self {
        let cfg = load_config();

        let inner: Box<dyn AudioBackend> = if cfg.bit_perfect_enabled {
            match Self::build_bit_perfect(&samples, &cfg.bit_perfect_device) {
                Ok(backend) => backend,
                Err(e) => {
                    eprintln!("[Backend] bit-perfect ALSA unavailable ({e}); falling back to CPAL");
                    Box::new(SymphoniaBackend::new(samples.clone()))
                }
            }
        } else {
            Box::new(SymphoniaBackend::new(samples.clone()))
        };

        Self {
            samples,
            inner,
            trim_start: AtomicF32::new(0.0),
            trim_end: AtomicF32::new(0.0),
            effective_duration: AtomicF32::new(f32::INFINITY),
            volume: AtomicF32::new(cfg.volume),
            low_gain: AtomicF32::new(1.0),
            mid_gain: AtomicF32::new(1.0),
            high_gain: AtomicF32::new(1.0),
            width: AtomicF32::new(1.0),
        }
    }

    /// Build the bit-perfect backend, probing the hardware for a usable
    /// device. Fails when the feature is disabled or no device can be opened.
    fn build_bit_perfect(
        samples: &SharedSamples,
        device: &str,
    ) -> Result<Box<dyn AudioBackend>, String> {
        #[cfg(all(target_os = "linux", feature = "bit-perfect-backend"))]
        {
            let resolved = crate::audio::alsa_backend::AlsaBackend::resolve_device(device)?;
            eprintln!("[Backend] bit-perfect ALSA enabled on device '{resolved}'");
            Ok(Box::new(crate::audio::alsa_backend::AlsaBackend::new(
                samples.clone(),
                resolved,
            )))
        }
        #[cfg(not(all(target_os = "linux", feature = "bit-perfect-backend")))]
        {
            let _ = (samples, device);
            Err("bit-perfect backend not compiled in".to_string())
        }
    }

    /// Switch the inner backend to CPAL, carrying over the current settings.
    fn fallback_to_cpal(&mut self) {
        let mut cpal = SymphoniaBackend::new(self.samples.clone());
        cpal.set_volume(self.volume.load(std::sync::atomic::Ordering::Relaxed));
        cpal.low_gain(self.low_gain.load(std::sync::atomic::Ordering::Relaxed));
        cpal.mid_gain(self.mid_gain.load(std::sync::atomic::Ordering::Relaxed));
        cpal.high_gain(self.high_gain.load(std::sync::atomic::Ordering::Relaxed));
        cpal.set_expander_width(self.width.load(std::sync::atomic::Ordering::Relaxed));
        cpal.set_trim(
            self.trim_start.load(std::sync::atomic::Ordering::Relaxed),
            self.trim_end.load(std::sync::atomic::Ordering::Relaxed),
            self.effective_duration.load(std::sync::atomic::Ordering::Relaxed),
        );
        self.inner = Box::new(cpal);
        eprintln!("[Backend] switched to CPAL backend");
    }
}

impl AudioBackend for BackendDispatcher {
    fn load(&mut self, track: &Track) -> Result<(), BackendError> {
        match self.inner.load(track) {
            Ok(()) => Ok(()),
            Err(e) => {
                if self.inner.supports_bit_perfect() {
                    eprintln!("[Backend] bit-perfect load failed ({e}); falling back to CPAL");
                    self.fallback_to_cpal();
                    self.inner.load(track)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn play(&mut self) {
        self.inner.play();
    }

    fn pause(&mut self) {
        self.inner.pause();
    }

    fn stop(&mut self) {
        self.inner.stop();
    }

    fn seek(&mut self, path: &Path, seconds: f32) {
        self.inner.seek(path, seconds);
    }

    fn position(&self) -> f32 {
        self.inner.position()
    }

    fn is_audible(&self) -> bool {
        self.inner.is_audible()
    }

    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }

    fn samples(&self) -> SharedSamples {
        self.inner.samples()
    }

    fn finished(&self) -> bool {
        self.inner.finished()
    }

    fn get_db_loudness(&self) -> (f32, f32) {
        self.inner.get_db_loudness()
    }

    fn set_volume(&self, volume: f32) {
        self.inner.set_volume(volume);
        self.volume.store(volume, std::sync::atomic::Ordering::Relaxed);
    }

    fn low_gain(&self, gain: f32) {
        self.inner.low_gain(gain);
        self.low_gain.store(gain, std::sync::atomic::Ordering::Relaxed);
    }

    fn mid_gain(&self, gain: f32) {
        self.inner.mid_gain(gain);
        self.mid_gain.store(gain, std::sync::atomic::Ordering::Relaxed);
    }

    fn high_gain(&self, gain: f32) {
        self.inner.high_gain(gain);
        self.high_gain.store(gain, std::sync::atomic::Ordering::Relaxed);
    }

    fn set_expander_width(&self, width: f32) {
        self.inner.set_expander_width(width);
        self.width.store(width, std::sync::atomic::Ordering::Relaxed);
    }

    fn set_trim(&mut self, start_secs: f32, end_secs: f32, effective_duration_secs: f32) {
        self.trim_start.store(start_secs, std::sync::atomic::Ordering::Relaxed);
        self.trim_end.store(end_secs, std::sync::atomic::Ordering::Relaxed);
        self.effective_duration.store(effective_duration_secs, std::sync::atomic::Ordering::Relaxed);
        self.inner.set_trim(start_secs, end_secs, effective_duration_secs);
    }

    fn trim_start(&self) -> f32 {
        self.trim_start.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn trim_end(&self) -> f32 {
        self.trim_end.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn prepare_next(&mut self, path: &Path, trim_start: f32) -> bool {
        self.inner.prepare_next(path, trim_start)
    }

    fn start_crossfade(&mut self, duration_ms: u32) {
        self.inner.start_crossfade(duration_ms);
    }

    fn is_crossfade_active(&self) -> bool {
        self.inner.is_crossfade_active()
    }

    fn is_next_finished(&self) -> bool {
        self.inner.is_next_finished()
    }

    fn crossfade_gains(&self) -> (f32, f32) {
        self.inner.crossfade_gains()
    }

    fn set_crossfade_gains(&self, out: f32, in_: f32) {
        self.inner.set_crossfade_gains(out, in_);
    }

    fn resume_crossfade(&self) {
        self.inner.resume_crossfade();
    }

    fn crossfade_swap(&mut self, xf_elapsed: f32) -> Option<PathBuf> {
        self.inner.crossfade_swap(xf_elapsed)
    }

    fn crossfade_abort(&mut self) {
        self.inner.crossfade_abort();
    }

    fn next_path(&self) -> Option<PathBuf> {
        self.inner.next_path()
    }

    fn consumer_depleted(&self) -> bool {
        self.inner.consumer_depleted()
    }

    fn supports_bit_perfect(&self) -> bool {
        self.inner.supports_bit_perfect()
    }
}
