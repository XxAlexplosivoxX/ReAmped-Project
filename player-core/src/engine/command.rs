use std::path::PathBuf;
use std::fmt;

use super::track::Track;

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

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Load(Vec<Track>),
    Samples,
    Play,
    Pause,
    Stop,
    Position,
    SetVolume(f32),
    Seek(f32),
    Next,
    Prev,
    ToggleShuffle,
    ToggleRepeat,
    ToggleRepeatOne,
    JumpTo(usize),
    JumpToPath(PathBuf),
    SetPlaylist(Vec<Track>),
    PlayIndex(usize),
    SetPlaylistAndPlayIndex(Vec<Track>, usize),
    ReloadCurrent,
    AleatoryFullRandom,
    SortBy(Options),
    GetPluginsData,
    SetGainBass(f32),
    SetGainMid(f32),
    SetGainHigh(f32),
    SetExpanderWidth(f32),
}
