use crate::{Track, player::Options};
use std::path::PathBuf;

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
    SetExpanderWidth(f32)
}
