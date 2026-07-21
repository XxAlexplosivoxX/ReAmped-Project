//! C-compatible data types for the FFI boundary.
//!
//! All types are annotated with `#[repr(C)]` to guarantee a stable ABI
//! matching the C calling convention.  String fields use raw pointer +
//! length pairs so that C hosts can read them without allocating.

/// Snapshot of the player state, copied from [`PlayerState`](crate::engine::state::PlayerState).
#[repr(C)]
pub struct CPlayerState {
    /// Whether audio is currently playing.
    pub playing: bool,
    /// Master volume (`0.0` – `1.0`).
    pub volume: f32,
    /// Current playback position in seconds.
    pub position: f32,
    /// Duration of the current track in seconds.
    pub duration: f32,
    /// Whether shuffle mode is active.
    pub shuffle: bool,
    /// Whether single-track repeat is active.
    pub repeat_one: bool,
    /// Whether playlist repeat is active.
    pub repeat: bool,
    /// Number of tracks in the playlist.
    pub playlist_len: usize,
    /// Index of the current track in the playlist.
    pub playlist_idx: usize,
    /// Sample rate of the current audio (Hz).
    pub sample_rate: f32,
}

/// A track entry passed across the FFI boundary.
///
/// String data points to UTF-8 bytes that are valid for the duration of the
/// call.  The caller **must not** mutate or free the pointed-to memory.
#[repr(C)]
pub struct CTrack {
    /// Pointer to the UTF-8 encoded file path.
    pub path: *const u8,
    /// Length of the path string in bytes.
    pub path_len: usize,
    /// Pointer to the UTF-8 encoded title.
    pub title: *const u8,
    /// Length of the title string in bytes.
    pub title_len: usize,
    /// Pointer to the UTF-8 encoded artist name.
    pub artist: *const u8,
    /// Length of the artist string in bytes.
    pub artist_len: usize,
    /// Track duration in seconds.
    pub duration: f32,
}

/// A playback command sent from C to the Rust engine.
///
/// This mirrors [`PlayerCommand`](crate::engine::command::PlayerCommand) with a
/// stable `repr(C)` layout.  Data-carrying variants use the Rust enum
/// representation (the discriminant is stored as an integer tag).
#[repr(C)]
pub enum CCommand {
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Stop playback and reset position.
    Stop,
    /// Skip to the next track.
    Next,
    /// Go back to the previous track.
    Prev,
    /// Toggle shuffle mode.
    ToggleShuffle,
    /// Toggle playlist-repeat mode.
    ToggleRepeat,
    /// Toggle single-track-repeat mode.
    ToggleRepeatOne,
    /// Set the volume (`0.0` – `1.0`).
    SetVolume(f32),
    /// Seek to a position in seconds.
    Seek(f32),
}

/// An event produced by the engine, forwarded to C callers.
#[repr(C)]
pub struct CEvent {
    /// Event kind identifier:
    ///
    /// | Kind | Variant |
    /// |------|---------|
    /// | `0` | `StateChanged` |
    /// | `1` | `TrackChanged` — `int_arg` holds the index |
    /// | `2` | `PlaylistChanged` |
    /// | `3` | `Loudness` — `float_arg` holds left dB |
    /// | `4` | `Error` |
    /// | `5` | `Shutdown` |
    pub kind: i32,
    /// Integer payload (varies by kind).
    pub int_arg: i64,
    /// Float payload (varies by kind).
    pub float_arg: f64,
}
