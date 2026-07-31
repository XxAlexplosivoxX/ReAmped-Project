//! Audio backend abstraction for the player.
//!
//! Defines the [`AudioBackend`] trait that all audio backends must implement,
//! along with sub-modules for crossfade math, visualisation data sharing, and
//! the Symphonia-based CPAL backend.

use std::path::Path;

use crate::Track;
use crate::audio::viz_source::SharedSamples;

pub mod crossfade;
pub mod symphonia_backend;
pub mod viz_source;

/// Unified audio backend trait.
///
/// Implementations must manage at least one decode thread feeding a ring-buffer
/// that the CPAL output callback drains.
pub trait AudioBackend {
    // ---- Lifecycle ----
    /// Load and begin decoding a track.
    ///
    /// Stops any current playback and aborts any pending crossfade before
    /// spawning a new decode thread and CPAL output stream.
    fn load(&mut self, track: &Track);
    /// Start or resume playback with a short fade-in.
    fn play(&mut self);
    /// Pause playback, freezing the position counter until [`Self::play`] is called again.
    fn pause(&mut self);
    /// Stop playback, join decode threads, and release the CPAL stream.
    fn stop(&mut self);
    /// Seek to `seconds` into the track and restart decoding from that position.
    fn seek(&mut self, path: &Path, seconds: f32);

    // ---- State queries ----
    /// Current playback position in seconds (adjusted for silence trim).
    fn position(&self) -> f32;
    /// Whether audio is currently being output. Stays `true` during a pause
    /// fade-out and only turns `false` once the output has gone silent.
    fn is_audible(&self) -> bool;
    /// Output sample rate of the CPAL stream in Hz.
    fn sample_rate(&self) -> f32;
    /// Shared buffer that the output callback fills with interleaved stereo samples
    /// for the UI to render as a waveform.
    fn samples(&self) -> SharedSamples;
    /// Whether the decode thread has reached end-of-file.
    fn finished(&self) -> bool;
    /// Instantaneous loudness in dB for the right and left channels.
    fn get_db_loudness(&self) -> (f32, f32);

    // ---- Volume / DSP ----
    /// Set the master output volume (`0.0` – `1.0`).
    fn set_volume(&self, volume: f32);
    /// Low-shelf EQ gain multiplier (`0.0` – `2.0`).
    fn low_gain(&self, gain: f32);
    /// Mid-band EQ gain multiplier (`0.0` – `2.0`).
    fn mid_gain(&self, gain: f32);
    /// High-shelf EQ gain multiplier (`0.0` – `2.0`).
    fn high_gain(&self, gain: f32);
    /// Stereo width for the mid/side expander (`0.0` = mono, `1.0` = original).
    fn set_expander_width(&self, width: f32);

    // ---- Silence trim ----
    /// Configure silence trimming: skip `start_secs` from the beginning and
    /// stop after `total_output_frames` frames.
    fn set_trim(&mut self, start_secs: f32, end_secs: f32, total_output_frames: u32);
    /// Start trim offset in seconds.
    fn trim_start(&self) -> f32;
    /// End trim offset in seconds.
    fn trim_end(&self) -> f32;

    // ---- Crossfade primitives ----
    /// Start decoding the next track into a secondary ring-buffer.
    /// Seeks to `trim_start` so only the effective audio is decoded.
    fn prepare_next(&mut self, path: &Path, trim_start: f32);

    /// Begin crossfade mixing in the CPAL callback.
    /// Resets the micro-fade counter.
    fn start_crossfade(&mut self, duration_ms: u32);

    /// Whether the callback is currently crossfade-mixing.
    fn is_crossfade_active(&self) -> bool;

    /// Has the next-track decode thread reached EOF?
    fn is_next_finished(&self) -> bool;

    /// Current crossfade gains: `(out_gain, in_gain)`.
    fn crossfade_gains(&self) -> (f32, f32);

    /// Override crossfade gains (used when resuming from a pause during fade).
    fn set_crossfade_gains(&self, out: f32, in_: f32);

    /// Resume crossfade mixing in the audio callback after a pause.
    ///
    /// The gains are expected to have been restored via
    /// [`set_crossfade_gains`] already; this method only flags the
    /// `xfade_active` atomic back to `true` so the callback's mixing
    /// branch is re-entered.
    fn resume_crossfade(&self);

    /// Promote the next track to primary. Stops the old decode thread,
    /// swaps consumers, resets crossfade state.
    /// `xf_elapsed` is how many seconds of the new track have already played
    /// during the crossfade — the position offset is adjusted accordingly.
    /// Returns the path of the newly-promoted track.
    fn crossfade_swap(&mut self, xf_elapsed: f32) -> Option<std::path::PathBuf>;

    /// Immediately stop the next-track decode and clear its consumer.
    fn crossfade_abort(&mut self);

    /// The path of the currently prepared (next) track, if any.
    fn next_path(&self) -> Option<std::path::PathBuf>;

    /// Has the current track stopped producing samples?
    fn consumer_depleted(&self) -> bool;
}
