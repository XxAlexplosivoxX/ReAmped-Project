#[repr(C)]
pub struct CPlayerState {
    pub playing: bool,
    pub volume: f32,
    pub position: f32,
    pub duration: f32,
    pub shuffle: bool,
    pub repeat_one: bool,
    pub repeat: bool,
    pub playlist_len: usize,
    pub playlist_idx: usize,
    pub sample_rate: f32,
}

#[repr(C)]
pub struct CTrack {
    pub path: *const u8,
    pub path_len: usize,
    pub title: *const u8,
    pub title_len: usize,
    pub artist: *const u8,
    pub artist_len: usize,
    pub duration: f32,
}

#[repr(C)]
pub enum CCommand {
    Play,
    Pause,
    Stop,
    Next,
    Prev,
    ToggleShuffle,
    ToggleRepeat,
    ToggleRepeatOne,
    SetVolume(f32),
    Seek(f32),
}

#[repr(C)]
pub struct CEvent {
    pub kind: i32,
    pub int_arg: i64,
    pub float_arg: f64,
}
