//! Crossfade utilities for gapless track transitions.
//!
//! Provides equal-power gain computation ([`equal_power_gains`]), a micro-fade
//! mechanism to prevent clicks on phase discontinuity, and the
//! [`CrossfadePhase`] state machine that drives the player's transition logic.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Equal-power crossfade gains.
/// `t` ∈ [0, 1], returns `(out_gain, in_gain)`.
/// - out_gain = cos(t · π/2)²  (fade-out of outgoing track)
/// - in_gain  = sin(t · π/2)²  (fade-in of incoming track)
///
/// Guarantees out² + in² = 1 at every t (constant perceived power).
pub fn equal_power_gains(t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    let angle = t * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    (cos * cos, sin * sin)
}

/// Duration of the micro-fade applied to the incoming track's first samples
/// to prevent clicks from phase discontinuity (in milliseconds).
pub const MICRO_FADE_MS: f32 = 4.0;

/// Number of frames the micro-fade spans at a given sample rate.
pub fn micro_fade_frame_count(sample_rate_hz: f32) -> u32 {
    ((MICRO_FADE_MS / 1000.0) * sample_rate_hz).max(1.0) as u32
}

/// Micro-fade multiplier for the incoming track.
/// Linearly ramps from 0 to 1 over `total` frames.
pub fn micro_fade_multiplier(frame_index: u32, total: u32) -> f32 {
    if frame_index >= total {
        1.0
    } else {
        frame_index as f32 / total as f32
    }
}

/// Resets the micro-fade frame counter (call when a crossfade starts).
pub fn reset_micro_fade(counter: &Arc<AtomicU32>) {
    counter.store(0, Ordering::SeqCst);
}

/// Advances the micro-fade counter and returns the current multiplier.
/// Should be called once per output frame during crossfade active.
/// Clamps at `total` and stays at 1.0 thereafter.
pub fn advance_micro_fade(counter: &Arc<AtomicU32>, total: u32) -> f32 {
    let cur = counter.load(Ordering::Relaxed);
    if cur >= total {
        return 1.0;
    }
    let next = cur + 1;
    counter.store(next.min(total), Ordering::Relaxed);
    micro_fade_multiplier(cur, total)
}

// ---------------------------------------------------------------------------
// State machine for the player thread
// ---------------------------------------------------------------------------

/// State machine for crossfade transitions between two tracks.
///
/// Tracks the lifecycle from idle → preparing (next track decoding) → fading
/// (both tracks mixing in the callback) or paused-fading (user paused mid-fade).
#[derive(Debug, Clone, PartialEq)]
pub enum CrossfadePhase {
    /// No crossfade activity at all.
    Idle,
    /// Next track is being decoded (ringbuffer filling).
    Preparing {
        next_index: usize,
    },
    /// Both tracks are mixing in the CPAL callback.
    Fading {
        next_index: usize,
        fade_start: Instant,
        fade_dur_secs: f32,
        /// Whether we have already signalled the UI to switch at t > 0.5
        ui_switched: bool,
    },
    /// User paused while a crossfade was active — gains are frozen.
    PausedFading {
        next_index: usize,
        saved_out: f32,
        saved_in: f32,
        fade_dur_secs: f32,
        elapsed_secs: f32,
    },
}

impl CrossfadePhase {
    /// Returns `true` while a crossfade is mixing (or paused mid-fade).
    pub fn is_active(&self) -> bool {
        matches!(self, CrossfadePhase::Fading { .. } | CrossfadePhase::PausedFading { .. })
    }

    /// Returns `true` if a next-track has been prepared (decoding or mixing).
    pub fn has_prepared(&self) -> bool {
        matches!(self, CrossfadePhase::Preparing { .. } | CrossfadePhase::Fading { .. } | CrossfadePhase::PausedFading { .. })
    }

    /// The playlist index of the incoming (next) track, if one has been prepared.
    pub fn next_index(&self) -> Option<usize> {
        match self {
            CrossfadePhase::Preparing { next_index } => Some(*next_index),
            CrossfadePhase::Fading { next_index, .. } => Some(*next_index),
            CrossfadePhase::PausedFading { next_index, .. } => Some(*next_index),
            CrossfadePhase::Idle => None,
        }
    }

    /// Maximum fade time for short tracks: never exceed 50 % of effective duration.
    pub fn clamp_duration(requested_secs: f32, effective_dur_a: f32, effective_dur_b: f32) -> f32 {
        let half_shortest = effective_dur_a.min(effective_dur_b) * 0.5;
        requested_secs.min(half_shortest).max(0.5)
    }
}
