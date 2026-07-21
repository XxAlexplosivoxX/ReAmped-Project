//! Event types and an [`EventBus`] for observer-style notifications.
//!
//! The engine publishes [`Event`]s to all subscribed receivers whenever
//! significant state changes occur (track switch, playback stopped, error, …).
//! This decouples the audio thread from UI or other consumers.

use std::sync::{Arc, Mutex, mpsc};

use super::error::PlayerError;

/// Events emitted by the audio engine.
///
/// Each variant carries the data needed for a UI or logging subscriber to react.
#[derive(Debug, Clone)]
pub enum Event {
    /// Play/pause/stop state has changed.
    StateChanged,
    /// The currently playing track has changed (carries new index).
    TrackChanged(usize),
    /// The playlist has been replaced or reordered.
    PlaylistChanged,
    /// Instantaneous loudness levels in dB (left, right).
    Loudness(f32, f32),
    /// A non-fatal engine error occurred.
    Error(PlayerError),
    /// The audio thread is shutting down.
    Shutdown,
}

/// A simple publish/subscribe channel for [`Event`]s.
///
/// Clone the bus to share it; all clones refer to the same subscriber list.
/// Dead or dropped receivers are automatically pruned on the next
/// [`publish`](EventBus::publish).
#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<Event>>>>,
}

impl EventBus {
    /// Create an empty event bus with no subscribers.
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a new subscriber and return its receive endpoint.
    pub fn subscribe(&self) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }

    /// Broadcast an event to all current subscribers.
    ///
    /// Subscribers whose channel has been closed are removed.
    pub fn publish(&self, event: Event) {
        self.subscribers
            .lock()
            .unwrap()
            .retain(|tx| tx.send(event.clone()).is_ok());
    }
}
