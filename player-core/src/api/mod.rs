//! Public-facing API types for controlling the audio player.
//!
//! [`builder::PlayerBuilder`] provides a builder-pattern constructor and
//! [`handle::Player`] is the primary handle for sending commands, reading
//! playback state, and polling events.

pub mod handle;
pub mod builder;
