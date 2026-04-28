use crate::{Track, player::Options};

#[derive(Debug)]
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
    SetPlaylist(Vec<Track>),
    PlayIndex(usize),
    ReloadCurrent,
    AleatoryFullRandom,
    SortBy(Options),
    GetPluginsData,
    SetGainBass(f32),
    SetGainMid(f32),
    SetGainHigh(f32),
    SetExpanderWidth(f32)
}
