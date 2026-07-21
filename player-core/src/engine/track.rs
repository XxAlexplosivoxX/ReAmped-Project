//! The [`Track`] type — a single playable item in the playlist.

use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// A single playable track with metadata.
///
/// `Track` is the unit item in the playlist. It stores the file path and
/// enough metadata to display in a list without opening the file.
#[derive(PartialEq, PartialOrd, Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    /// Absolute or relative filesystem path to the audio file.
    pub path: PathBuf,
    /// Display title of the track.
    pub title: String,
    /// Artist name.
    pub artist: String,
    /// Total duration in seconds (from file tags or header probe).
    pub duration: f32,
}
