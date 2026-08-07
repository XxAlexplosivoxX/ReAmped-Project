//! Command types sent to the audio engine thread.
//!
//! The [`PlayerCommand`] enum is the single channel-based interface for
//! controlling playback. Each variant represents an atomic operation that the
//! audio loop processes in its event loop (see
//! `audio_loop`).

use std::path::PathBuf;
use std::fmt;

use super::track::Track;

/// Sort order for [`PlayerCommand::SortBy`].
#[derive(PartialEq, Debug, Clone)]
pub enum Options {
    /// Keep / restore the original playlist order.
    Normal,
    /// Sort tracks alphabetically by title.
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

/// Commands that can be sent to the audio engine thread.
///
/// All commands are submitted over an `mpsc::Receiver<PlayerCommand>` and
/// processed sequentially by `audio_loop`.
#[derive(Debug, Clone)]
pub enum PlayerCommand {
    /// Load a new set of tracks into the backend without playing.
    Load(Vec<Track>),
    /// Request the current audio sample data (loudness / viz).
    Samples,
    /// Start or resume playback.
    Play,
    /// Pause playback. If a crossfade is active, its gains are frozen
    /// ([`CrossfadePhase::PausedFading`](crate::audio::crossfade::CrossfadePhase::PausedFading)).
    Pause,
    /// Stop playback and reset the engine to an idle state.
    Stop,
    /// Query the current playback position.
    Position,
    /// Set the output volume (`0.0` … `1.0`).
    SetVolume(f32),
    /// Seek to a given time in seconds.
    Seek(f32),
    /// Skip to the next track (respecting shuffle / repeat).
    Next,
    /// Go back to the previous track (restart if > 3 s elapsed).
    Prev,
    /// Toggle shuffle mode on / off.
    ToggleShuffle,
    /// Toggle repeat-all mode on / off.
    ToggleRepeat,
    /// Toggle repeat-one mode on / off.
    ToggleRepeatOne,
    /// Jump to track at the given playlist index.
    JumpTo(usize),
    /// Jump to the track whose path matches the given [`PathBuf`].
    JumpToPath(PathBuf),
    /// Replace the entire playlist (no auto-play).
    SetPlaylist(Vec<Track>),
    /// Play the track at the given index immediately.
    PlayIndex(usize),
    /// Replace the playlist and start playback at the given index.
    SetPlaylistAndPlayIndex(Vec<Track>, usize),
    /// Reload the currently playing track from disk.
    ReloadCurrent,
    /// Fully randomise the playlist order in-place.
    AleatoryFullRandom,
    /// Sort the playlist by the given [`Options`].
    SortBy(Options),
    /// Request plugin data snapshot (EQ, expander, etc.).
    GetPluginsData,
    /// Set low (bass) EQ gain.
    SetGainBass(f32),
    /// Set mid EQ gain.
    SetGainMid(f32),
    /// Set high (treble) EQ gain.
    SetGainHigh(f32),
    /// Set the stereo expander width factor.
    SetExpanderWidth(f32),
    /// Reconfigure the output backend on the fly: enable/disable bit-perfect
    /// ALSA output and/or change the target device. The engine rebuilds the
    /// backend and reloads the current track so the change applies immediately.
    SetBitPerfectBackend { enabled: bool, device: String },
}
