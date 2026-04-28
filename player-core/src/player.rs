use atomic_float::AtomicF32;
use rand::rng;
use rand::seq::SliceRandom;
use std::sync::atomic::Ordering;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, Sender, channel},
};
use std::{fmt, thread, time::Duration};

use crate::audio::{AudioBackend, symphonia_backend::SymphoniaBackend};
use crate::{PlayerCommand, PlayerState, Track};
use crate::{
    audio::viz_source::SharedSamples,
    metadata::{CoverArt, default_cover, read_metadata},
};

#[derive(PartialEq, Debug, Clone)]
pub enum Options {
    Normal,
    Alphabetical,
}
impl fmt::Display for Options {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Options::Normal => write!(f, "Default"),
            Options::Alphabetical => write!(f, "Alphabetical"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    tx: Sender<PlayerCommand>,
    pub samples: SharedSamples,
    pub state: Arc<Mutex<PlayerState>>,
    pub db_l: Arc<AtomicF32>,
    pub db_r: Arc<AtomicF32>,
}

fn load_current(
    backend: &mut SymphoniaBackend,
    playlist: &Vec<Track>,
    index: usize,
    state: &Arc<Mutex<PlayerState>>,
) {
    if playlist.is_empty() {
        return;
    }

    let track = &playlist[index];
    backend.load(track);

    let metadata = read_metadata(&track.path);

    let mut s = state.lock().unwrap();

    s.metadata = Some(metadata.clone());
    s.current_track = metadata.title;
    s.duration = metadata.duration;
    s.cover = metadata.cover;
    s.position = 0.0;
    s.playing = true;
    s.playlist = playlist.clone();
    s.playlist_cpy = playlist.clone();
    s.playlist_idx = index;
}

impl Player {
    pub fn new(volume: f32) -> Self {
        let (tx, rx) = channel();
        let samples = Arc::new(Mutex::new(Vec::with_capacity(4096)));
        let state = Arc::new(Mutex::new(PlayerState {
            playing: false,
            volume: volume,
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
        }));
        let db_r = Arc::new(AtomicF32::new(-100.0));
        let db_l = Arc::new(AtomicF32::new(-100.0));
        let db_l_cln = db_l.clone();
        let db_r_cln = db_r.clone();

        thread::spawn({
            let samples = samples.clone();
            let state = state.clone();
            move || audio_loop(rx, samples, state, db_l_cln, db_r_cln)
        });
        Self {
            tx,
            samples,
            state,
            db_l,
            db_r,
        }
    }

    pub fn send(&self, cmd: PlayerCommand) {
        let _ = self.tx.send(cmd);
    }

    pub fn cover(&self) -> CoverArt {
        self.state.lock().unwrap().cover.clone()
    }

    pub fn is_playing(&self) -> bool {
        self.state.lock().unwrap().playing.clone()
    }

    pub fn position(&self) -> f32 {
        let pos = self.state.lock().unwrap().position.clone();
        pos
    }

    pub fn playlist(&self) -> Vec<Track> {
        self.state.lock().unwrap().playlist.clone()
    }

    pub fn playlist_idx(&self) -> usize {
        self.state.lock().unwrap().playlist_idx.clone()
    }

    pub fn get_loudness(&self) -> (f32, f32) {
        (
            self.db_l.load(Ordering::SeqCst),
            self.db_r.load(Ordering::SeqCst),
        )
    }
}

fn audio_loop(
    rx: Receiver<PlayerCommand>,
    samples: SharedSamples,
    state: Arc<Mutex<PlayerState>>,
    db_l: Arc<AtomicF32>,
    db_r: Arc<AtomicF32>,
) {
    let mut backend = SymphoniaBackend::new(samples);
    let mut playlist: Vec<Track> = Vec::new();
    let mut current_index: usize = 0;
    let mut shuffle = false;
    let mut repeat = false;
    let mut repeat_one = false;
    let mut rng = rng();
    let mut shuffled_indices: Vec<usize> = Vec::new();
    let mut shuffle_pos: usize = 0;

    loop {
        let playing = state.lock().unwrap().playing;

        if playing && backend.finished() {
            if repeat_one {
                load_current(&mut backend, &playlist, current_index, &state);
                backend.play();
                continue;
            }

            if shuffle {
                shuffle_pos += 1;

                if shuffle_pos >= shuffled_indices.len() {
                    if repeat {
                        shuffled_indices.shuffle(&mut rng);
                        shuffle_pos = 0;
                    } else {
                        state.lock().unwrap().playing = false;
                        continue;
                    }
                }

                current_index = shuffled_indices[shuffle_pos];
                load_current(&mut backend, &playlist, current_index, &state);
                backend.play();
                continue;
            }

            if current_index + 1 < playlist.len() {
                current_index += 1;
                load_current(&mut backend, &playlist, current_index, &state);
                backend.play();
            } else if repeat {
                current_index = 0;
                load_current(&mut backend, &playlist, current_index, &state);
                backend.play();
            } else {
                state.lock().unwrap().playing = false;
            }
        }
        if playing {
            let (l, r) = backend.get_db_loudness();
            db_l.store(l, Ordering::Relaxed);
            db_r.store(r, Ordering::Relaxed);
            let mut s = state.lock().unwrap();
            s.position = backend.position();
        } else {
            db_l.store(-100.0, Ordering::Relaxed);
            db_r.store(-100.0, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(16));
        }

        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(cmd) => match cmd {
                PlayerCommand::SetPlaylist(list) => {
                    playlist = list;
                    shuffled_indices = (0..playlist.len()).collect();
                    shuffled_indices.shuffle(&mut rng);
                    shuffle_pos = 0;

                    state.lock().unwrap().playlist = playlist.clone();
                    state.lock().unwrap().playlist_cpy = playlist.clone();
                }
                PlayerCommand::PlayIndex(index) => {
                    if index >= playlist.len() {
                        continue;
                    }

                    current_index = index;

                    if shuffle {
                        if let Some(pos) = shuffled_indices.iter().position(|&i| i == index) {
                            shuffle_pos = pos;
                        }
                    }

                    load_current(&mut backend, &playlist, current_index, &state);
                    backend.play();

                    state.lock().unwrap().playing = true;
                }

                // PlayerCommand::Load(list) => {
                //     playlist = list.clone();
                //     current_index = 0;

                //     shuffled_indices = (0..playlist.len()).collect();
                //     shuffled_indices.shuffle(&mut rng);
                //     shuffle_pos = 0;

                //     load_current(&mut backend, &playlist, current_index, &state);
                // }
                PlayerCommand::Play => {
                    backend.play();
                    let mut s = state.lock().unwrap();
                    s.playing = true;
                }

                PlayerCommand::Pause => {
                    backend.pause();
                    let mut s = state.lock().unwrap();
                    s.playing = false;
                }

                PlayerCommand::Next => {
                    if shuffle {
                        shuffle_pos += 1;

                        if shuffle_pos >= shuffled_indices.len() {
                            if repeat {
                                shuffled_indices.shuffle(&mut rng);
                                shuffle_pos = 0;
                            } else {
                                state.lock().unwrap().playing = false;
                                continue;
                            }
                        }

                        current_index = shuffled_indices[shuffle_pos];
                    } else if current_index + 1 < playlist.len() {
                        current_index += 1;
                    } else if repeat {
                        current_index = 0;
                    } else {
                        state.lock().unwrap().playing = false;
                        continue;
                    }

                    load_current(&mut backend, &playlist, current_index, &state);
                    backend.play();
                }

                PlayerCommand::Prev => {
                    if backend.position() > 3.0 {
                        backend.seek(&playlist[current_index].path, 0.0);
                    } else if current_index > 0 {
                        current_index -= 1;
                        load_current(&mut backend, &playlist, current_index, &state);
                    }
                }

                PlayerCommand::Stop => {
                    backend.stop();

                    let mut s = state.lock().unwrap();
                    s.cover = default_cover();
                    s.current_track = "--- Stopped ---".into();
                    s.duration = 0.0;
                    s.playing = false;
                    s.position = 0.0;
                    s.metadata = None;
                    s.playing = false;
                }

                PlayerCommand::Seek(t) => {
                    backend.seek(&playlist[current_index].path, t);
                    let mut s = state.lock().unwrap();
                    s.position = t;
                    s.playing = true;
                }

                PlayerCommand::SetVolume(v) => {
                    backend.set_volume(v);
                    state.lock().unwrap().volume = v;
                }

                PlayerCommand::ToggleShuffle => {
                    let mut s = state.lock().unwrap();
                    shuffle = !shuffle;
                    s.shuffle = shuffle;
                    repeat_one = false;
                    s.repeat_one = repeat_one;
                    if shuffle {
                        // reconstruir SIEMPRE desde cero
                        shuffled_indices = (0..playlist.len()).collect();
                        shuffled_indices.shuffle(&mut rng);

                        // mover el track actual al inicio
                        if let Some(pos) = shuffled_indices.iter().position(|&i| i == current_index)
                        {
                            shuffled_indices.swap(0, pos);
                        }

                        shuffle_pos = 0;
                    }
                }

                PlayerCommand::ToggleRepeat => {
                    let mut s = state.lock().unwrap();
                    repeat = !repeat;
                    repeat_one = false;
                    s.repeat = repeat;
                    s.repeat_one = repeat_one;
                }

                PlayerCommand::ToggleRepeatOne => {
                    let mut s = state.lock().unwrap();
                    repeat_one = !repeat_one;
                    repeat = false;
                    s.repeat = repeat;
                    s.repeat_one = repeat_one;
                }

                PlayerCommand::SortBy(op) => {
                    match op {
                        Options::Normal => {
                            playlist = state.lock().unwrap().playlist_cpy.clone();
                        }
                        Options::Alphabetical => {
                            playlist.sort_by(|a, b| a.title.cmp(&b.title));
                        }
                    }

                    // sincronizar state
                    let mut s = state.lock().unwrap();
                    s.playlist = playlist.clone();
                }

                PlayerCommand::AleatoryFullRandom => {
                    playlist.shuffle(&mut rng);

                    let mut s = state.lock().unwrap();
                    s.playlist = playlist.clone();
                }

                PlayerCommand::JumpTo(index) => {
                    if index >= playlist.len() {
                        continue;
                    }

                    current_index = index;

                    if shuffle {
                        // sincronizar shuffle_pos con el índice real
                        if let Some(pos) = shuffled_indices.iter().position(|&i| i == index) {
                            shuffle_pos = pos;
                        } else {
                            // por seguridad extrema
                            shuffled_indices.push(index);
                            shuffle_pos = shuffled_indices.len() - 1;
                        }
                    }

                    load_current(&mut backend, &playlist, current_index, &state);
                    backend.play();

                    let mut s = state.lock().unwrap();
                    s.playing = true;
                }

                PlayerCommand::SetGainBass(gain) => {
                    backend.low_gain(gain);
                }

                PlayerCommand::SetGainMid(gain) => {
                    backend.mid_gain(gain);
                }

                PlayerCommand::SetGainHigh(gain) => {
                    backend.high_gain(gain);
                }

                PlayerCommand::SetExpanderWidth(width) => {
                    backend.set_expander_width(width);
                }
                _ => {}
            },

            Err(RecvTimeoutError::Timeout) => {
                // continua, pos
            }

            Err(RecvTimeoutError::Disconnected) => {
                println!("Audio thread closed cleanly");
                break;
            }
        }
    }
}
