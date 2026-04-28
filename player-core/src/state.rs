use crate::{metadata::{CoverArt, Metadata}, Track};

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub playing: bool,
    pub volume: f32,
    pub position: f32,
    pub duration: f32,
    pub current_track: String,
    pub cover: CoverArt,
    pub metadata: Option<Metadata>,
    pub shuffle: bool,
    pub repeat_one: bool,
    pub repeat: bool,
    pub playlist: Vec<Track>,
    pub playlist_cpy: Vec<Track>,
    pub playlist_idx: usize
}

