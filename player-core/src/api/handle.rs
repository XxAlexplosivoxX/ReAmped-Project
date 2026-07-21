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
    pub fn send(&self, cmd: PlayerCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn cover(&self) -> CoverArt {
        self.state.lock().unwrap().cover.clone()
    }

    pub fn is_playing(&self) -> bool {
        self.state.lock().unwrap().playing
    }

    pub fn position(&self) -> f32 {
        self.state.lock().unwrap().position
    }

    pub fn playlist(&self) -> Vec<Track> {
        self.state.lock().unwrap().playlist.clone()
    }

    pub fn playlist_idx(&self) -> usize {
        self.state.lock().unwrap().playlist_idx
    }

    pub fn get_loudness(&self) -> (f32, f32) {
        use std::sync::atomic::Ordering;
        (
            self.db_l.load(Ordering::SeqCst),
            self.db_r.load(Ordering::SeqCst),
        )
    }

    pub fn get_sample_rate(&self) -> f32 {
        self.state.lock().unwrap().sample_rate
    }

    pub fn samples(&self) -> &SharedSamples {
        &self.samples
    }

    pub fn metadata(&self) -> Option<Metadata> {
        self.state.lock().unwrap().metadata.clone()
    }

    pub fn duration(&self) -> f32 {
        self.state.lock().unwrap().duration
    }

    pub fn volume(&self) -> f32 {
        self.state.lock().unwrap().volume
    }

    pub fn shuffle(&self) -> bool {
        self.state.lock().unwrap().shuffle
    }

    pub fn repeat(&self) -> bool {
        self.state.lock().unwrap().repeat
    }

    pub fn repeat_one(&self) -> bool {
        self.state.lock().unwrap().repeat_one
    }

    pub fn try_recv_event(&self) -> Option<Event> {
        self.event_rx.lock().unwrap().try_recv().ok()
    }
}
