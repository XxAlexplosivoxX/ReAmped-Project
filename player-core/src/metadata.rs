//! Audio file metadata and cover-art extraction.
//!
//! Uses the [lofty] crate to read tags from common audio formats (MP3, FLAC,
//! Ogg Vorbis, Opus, AAC, WAV, etc.).  Cover art is returned as raw bytes
//! together with the MIME type.

use lofty::file::TaggedFileExt;
use lofty::picture::MimeType;
use lofty::prelude::AudioFile;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::path::Path;

/// Parsed metadata for a single audio track.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// Track title (falls back to `"Unknown"`).
    pub title: String,
    /// Artist name (falls back to `"Unknown"`).
    pub artist: String,
    /// Duration in seconds (from stream properties).
    pub duration: f32,
    /// Embedded cover art (may be the default placeholder).
    pub cover: CoverArt,
}

/// Raw cover-art image with MIME type information.
#[derive(Clone, Debug)]
pub struct CoverArt {
    /// Decoded image bytes (PNG, JPEG, etc.).
    pub data: Vec<u8>,
    /// MIME type of the image data (e.g. `image/png`, `image/jpeg`).
    pub mime: MimeType,
}

/// Returns the default cover-art placeholder image.
///
/// The image is compiled into the binary from `assets/default.png`.
pub fn default_cover() -> CoverArt {
    const DEFAULT_COVER: &[u8] = include_bytes!("../../assets/default.png");
    CoverArt {
        data: DEFAULT_COVER.to_vec(),
        mime: MimeType::Png,
    }
}

/// Reads metadata and cover art from an audio file.
///
/// Returns `None` when the file cannot be opened or no tags are found.
/// The cover art falls back to [`default_cover`] when the file has no
/// embedded pictures.
pub fn read_metadata(path: &Path) -> Option<Metadata> {
    let tagged = Probe::open(path)
        .and_then(|p| p.read())
        .ok()?;

    let duration = tagged.properties().duration().as_secs_f32();

    let tag = tagged.first_tag();

    let cover = tag
        .and_then(|t| t.pictures().first())
        .and_then(|p| {
            if p.data().is_empty() {
                return None;
            }

            Some(CoverArt {
                data: p.data().to_vec(),
                mime: p.mime_type().unwrap_or(&MimeType::Png).clone(),
            })
        })
        .unwrap_or_else(default_cover);

    Some(Metadata {
        title: tag
            .and_then(|t| t.title())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".into()),

        artist: tag
            .and_then(|t| t.artist())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".into()),

        duration,
        cover,
    })
}
