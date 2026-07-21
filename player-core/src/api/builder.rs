//! Builder-pattern constructor for [`Player`].
//!
//! [`PlayerBuilder`] initialises the engine internals: the mpsc command
//! channel, the shared [`PlayerState`], the ring buffer, loudness metres,
//! and the event bus.  Calling [`build`](PlayerBuilder::build) spawns the
//! audio decoder thread and returns a ready-to-use [`Player`] handle.

use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;

use atomic_float::AtomicF32;

use super::handle::Player;
use crate::engine::event::EventBus;
use crate::engine::player::spawn_audio_thread;
use crate::engine::state::PlayerState;
use crate::metadata::default_cover;

/// Builder for constructing a [`Player`] instance.
///
/// # Example
///
/// ```no_run
/// use player_core::PlayerBuilder;
///
/// let player = PlayerBuilder::new()
///     .with_volume(0.8)
///     .build();
/// ```
pub struct PlayerBuilder {
    volume: f32,
}

impl Default for PlayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerBuilder {
    /// Creates a new builder with default volume (`1.0`).
    pub fn new() -> Self {
        Self { volume: 1.0 }
    }

    /// Sets the initial playback volume (clamped by the engine to `[0.0, 1.0]`).
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    /// Builds the player, spawning the audio thread and returning a [`Player`] handle.
    ///
    /// The spawned thread begins in a paused state, waiting for the first
    /// `PlayerCommand::Play` to be sent.
    pub fn build(self) -> Player {
        let (cmd_tx, cmd_rx) = channel();
        let samples = Arc::new(Mutex::new(Vec::with_capacity(4096)));
        let state = Arc::new(Mutex::new(PlayerState {
            playing: false,
            volume: self.volume,
            position: 0.0,
            duration: 0.0,
            current_track: String::from("None"),
            metadata: None,
            cover: default_cover(),
            shuffle: false,
            repeat_one: false,
            repeat: false,
            playlist: Vec::new(),
            playlist_cpy: Vec::new(),
            playlist_idx: 0,
            sample_rate: 41000.0,
        }));
        let db_l = Arc::new(AtomicF32::new(-100.0));
        let db_r = Arc::new(AtomicF32::new(-100.0));

        let event_bus = EventBus::new();
        let event_rx = event_bus.subscribe();

        spawn_audio_thread(
            cmd_rx,
            samples.clone(),
            state.clone(),
            db_l.clone(),
            db_r.clone(),
            event_bus,
        );

        Player {
            cmd_tx,
            samples,
            state,
            db_l,
            db_r,
            event_rx: Arc::new(Mutex::new(event_rx)),
        }
    }
}
