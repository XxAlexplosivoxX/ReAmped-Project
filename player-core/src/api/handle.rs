//! Main client-side handle for controlling the player.
//!
//! [`Player`] is a cheaply clonable handle that can be sent between threads.
//! Use it to dispatch commands, read the current playback state, access
//! decoded audio samples for visualisation, and poll engine events.

use atomic_float::AtomicF32;
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, Sender},
};

use crate::engine::{
    command::PlayerCommand,
    event::Event,
    state::PlayerState,
    track::Track,
};
use crate::audio::viz_source::SharedSamples;
use crate::metadata::{CoverArt, Metadata};

/// The primary handle to a running player engine.
///
/// `Player` is [`Clone`] and [`Send`], so it can be shared freely across
/// threads.  All state readers acquire the internal mutex temporarily and
/// return a snapshot — they are safe to call from any context.
#[derive(Clone)]
pub struct Player {
    pub(crate) cmd_tx: Sender<PlayerCommand>,
    pub(crate) samples: SharedSamples,
    pub(crate) state: Arc<Mutex<PlayerState>>,
    pub(crate) db_l: Arc<AtomicF32>,
    pub(crate) db_r: Arc<AtomicF32>,
    pub(crate) event_rx: Arc<Mutex<Receiver<Event>>>,
}

impl Player {
    /// Sends a [`PlayerCommand`] to the engine thread for execution.
    ///
    /// The command is enqueued on a bounded mpsc channel.  Returns
    /// immediately; the engine processes it asynchronously.
    pub fn send(&self, cmd: PlayerCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Returns a snapshot of the current cover art.
    pub fn cover(&self) -> CoverArt {
        self.state.lock().unwrap().cover.clone()
    }

    /// Returns `true` when audio is actively playing.
    pub fn is_playing(&self) -> bool {
        self.state.lock().unwrap().playing
    }

    /// Current playback position in seconds.
    pub fn position(&self) -> f32 {
        self.state.lock().unwrap().position
    }

    /// Returns a cloned copy of the current playlist.
    pub fn playlist(&self) -> Vec<Track> {
        self.state.lock().unwrap().playlist.clone()
    }

    /// Index of the currently active track in the playlist.
    pub fn playlist_idx(&self) -> usize {
        self.state.lock().unwrap().playlist_idx
    }

    /// Loudness level in dB for the left and right channels.
    ///
    /// Values are read atomically and are updated by the audio thread at
    /// (roughly) display refresh rate.
    pub fn get_loudness(&self) -> (f32, f32) {
        use std::sync::atomic::Ordering;
        (
            self.db_l.load(Ordering::SeqCst),
            self.db_r.load(Ordering::SeqCst),
        )
    }

    /// Sample rate of the current audio stream in Hz.
    pub fn get_sample_rate(&self) -> f32 {
        self.state.lock().unwrap().sample_rate
    }

    /// Shared reference to the decoded sample buffer for visualisation.
    ///
    /// The buffer is filled by the audio thread and read by the
    /// visualisation subsystem.
    pub fn samples(&self) -> &SharedSamples {
        &self.samples
    }

    /// Metadata for the current track, if available.
    pub fn metadata(&self) -> Option<Metadata> {
        self.state.lock().unwrap().metadata.clone()
    }

    /// Duration of the current track in seconds.
    pub fn duration(&self) -> f32 {
        self.state.lock().unwrap().duration
    }

    /// Current volume level in the `[0.0, 1.0]` range.
    pub fn volume(&self) -> f32 {
        self.state.lock().unwrap().volume
    }

    /// Whether shuffle mode is enabled.
    pub fn shuffle(&self) -> bool {
        self.state.lock().unwrap().shuffle
    }

    /// Whether playlist-repeat mode is enabled.
    pub fn repeat(&self) -> bool {
        self.state.lock().unwrap().repeat
    }

    /// Whether single-track-repeat mode is enabled.
    pub fn repeat_one(&self) -> bool {
        self.state.lock().unwrap().repeat_one
    }

    /// Attempts to receive an engine [`Event`] without blocking.
    ///
    /// Returns `None` when no events are pending.  Callers should poll this
    /// method in their event loop rather than blocking indefinitely.
    pub fn try_recv_event(&self) -> Option<Event> {
        self.event_rx.lock().unwrap().try_recv().ok()
    }
}
