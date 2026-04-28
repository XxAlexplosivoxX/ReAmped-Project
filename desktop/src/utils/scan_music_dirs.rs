use std::path::PathBuf;
use player_core::{metadata::read_metadata, track::Track};
use walkdir::WalkDir;


const AUDIO_EXTS: &[&str] = &["mp3", "wav", "flac", "ogg", "opus", "m4a", "aac"];

pub fn scan_music_dirs(dirs: &[PathBuf]) -> Vec<Track> {
    let mut tracks = Vec::new();

    for dir in dirs {
        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            if !AUDIO_EXTS.contains(&ext.as_str()) {
                continue;
            }

            let metadata = read_metadata(path);

            tracks.push(Track {
                path: path.to_path_buf(),
                title: metadata.title,
                artist: metadata.artist,
                duration: metadata.duration,
            });
        }
    }

    tracks
}