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
    fn load(&mut self, track: &Track);
    fn play(&mut self);
    fn pause(&mut self);
    fn stop(&mut self);
    fn seek(&mut self, path: &Path, seconds: f32);

    // ---- State queries ----
    fn position(&self) -> f32;
    fn sample_rate(&self) -> f32;
    fn samples(&self) -> SharedSamples;
    fn finished(&self) -> bool;
    fn get_db_loudness(&self) -> (f32, f32);

    // ---- Volume / DSP ----
    fn set_volume(&self, volume: f32);
    fn low_gain(&self, gain: f32);
    fn mid_gain(&self, gain: f32);
    fn high_gain(&self, gain: f32);
    fn set_expander_width(&self, width: f32);

    // ---- Silence trim ----
    fn set_trim(&mut self, start_secs: f32, end_secs: f32, total_output_frames: u32);
    fn trim_start(&self) -> f32;
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
