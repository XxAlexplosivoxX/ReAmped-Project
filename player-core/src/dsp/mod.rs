//! Digital signal processing primitives for the audio playback pipeline.
//!
//! Provides building blocks for audio processing:
//! * [`mini_eq`] — three-band equalizer using cascaded biquad filters
//! * [`xpander`] — mid/side stereo width processor
//! * [`db_meter`] — RMS-based level metering with ballistic response
//! * [`silence_detector`] — leading/trailing silence detection for track trimming

pub mod mini_eq;
pub mod xpander;
pub mod db_meter;
pub mod silence_detector;