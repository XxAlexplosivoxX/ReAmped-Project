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
    #[serde(default)]
    pub album: String,
    /// Total duration in seconds (from file tags or header probe).
    pub duration: f32,
}

#[cfg(test)]
mod tests {
    use super::Track;

    #[test]
    fn deserialize_track_without_album() {
        let track: Track = serde_json::from_str(
            r#"{"path":"song.flac","title":"Song","artist":"Artist","duration":120.0}"#,
        ).unwrap();

        assert_eq!(track.album, "");
        assert_eq!(track.title, "Song");
        assert_eq!(track.artist, "Artist");
        assert_eq!(track.duration, 120.0);
    }

    #[test]
    fn album_survives_serialization() {
        for album in ["", "Álbum"] {
            let track = Track {
                path: "song.flac".into(),
                title: "Song".into(),
                artist: "Artist".into(),
                album: album.into(),
                duration: 120.0,
            };
            let json = serde_json::to_string(&track).unwrap();
            let restored: Track = serde_json::from_str(&json).unwrap();

            assert_eq!(restored, track);
        }
    }
}
