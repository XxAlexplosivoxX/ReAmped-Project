use lofty::file::TaggedFileExt;
use lofty::picture::MimeType;
use lofty::prelude::AudioFile;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub duration: f32,
    pub cover: CoverArt,
}

#[derive(Clone, Debug)]
pub struct CoverArt {
    pub data: Vec<u8>,
    pub mime: MimeType,
}

pub fn default_cover() -> CoverArt {
const DEFAULT_COVER: &[u8] = include_bytes!("../../assets/default.png");
CoverArt {
    data: DEFAULT_COVER.to_vec(),
        mime: MimeType::Png,
    }
}

pub fn read_metadata(path: &Path) -> Metadata {
    

    let tagged = Probe::open(path)
        .and_then(|p| p.read())
        .expect("Failed to read audio file");

    let duration = tagged
        .properties()
        .duration()
        .as_secs_f32();

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

    Metadata {
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
    }
}
