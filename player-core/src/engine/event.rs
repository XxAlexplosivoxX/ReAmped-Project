use std::sync::{Arc, Mutex, mpsc};

use super::error::PlayerError;

#[derive(Debug, Clone)]
pub enum Event {
    StateChanged,
    TrackChanged(usize),
    PlaylistChanged,
    Loudness(f32, f32),
    Error(PlayerError),
    Shutdown,
}

#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<Event>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn subscribe(&self) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }

    pub fn publish(&self, event: Event) {
        self.subscribers
            .lock()
            .unwrap()
            .retain(|tx| tx.send(event.clone()).is_ok());
    }
}
