use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::cache::{CacheEntry, TrackCache};
use player_core::{metadata::read_metadata, Track};
use walkdir::WalkDir;

const AUDIO_EXTS: &[&str] = &[
    "mp3", "mpa",
    "wav", "wave", "aif", "aiff", "aifc",
    "flac", "ogg", "oga", "vorbis",
    "mka", "mkv", "webm",
];

fn fallback_track_info(path: &Path) -> (String, String, f32) {
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    (title, String::from("Unknown"), 0.0)
}

fn has_audio_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    AUDIO_EXTS.contains(&ext.as_str())
}

fn is_audio_file(path: &Path) -> bool {
    let ext_known = has_audio_extension(path);

    match infer::get_from_path(path) {
        Ok(Some(file_type)) if file_type.mime_type().starts_with("audio/") => true,
        Ok(Some(file_type)) if file_type.mime_type() == "application/ogg" => true,
        Ok(Some(_file_type)) => ext_known,
        _ => ext_known,
    }
}

fn push_track_if_audio(path: &Path, tracks: &mut Vec<Track>, cache: &mut TrackCache) {
    if !is_audio_file(path) {
        return;
    }

    if cache.is_valid(path) {
        if let Some(cached) = cache.get(path) {
            tracks.push(Track {
                path: path.to_path_buf(),
                title: cached.title, 
                artist: cached.artist,
                duration: cached.duration,
            });
            return;
        }
    }

    let (fallback_title, fallback_artist, fallback_duration) = fallback_track_info(path);

    if let Some(metadata) = read_metadata(path) {
        if let Ok(file_meta) = fs::metadata(path) {
            if let Ok(modified) = file_meta.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let modified_time = duration.as_secs();
                    let file_size = file_meta.len();
                    cache.insert(
                        path.to_path_buf(),
                        CacheEntry {
                            title: metadata.title.clone(),
                            artist: metadata.artist.clone(),
                            duration: metadata.duration,
                            modified_time,
                            file_size,
                        },
                    );
                }
            }
        }
        tracks.push(Track {
            path: path.to_path_buf(),
            title: metadata.title,
            artist: metadata.artist,
            duration: metadata.duration,
        });
    } else {
        if let Ok(file_meta) = fs::metadata(path) {
            if let Ok(modified) = file_meta.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let modified_time = duration.as_secs();
                    let file_size = file_meta.len();
                    cache.insert(
                        path.to_path_buf(),
                        CacheEntry {
                            title: fallback_title.clone(),
                            artist: fallback_artist.clone(),
                            duration: fallback_duration,
                            modified_time,
                            file_size,
                        },
                    );
                }
            }
        }

        tracks.push(Track {
            path: path.to_path_buf(),
            title: fallback_title,
            artist: fallback_artist,
            duration: fallback_duration,
        });
    }
}

pub fn scan_music_dirs(dirs: &[PathBuf]) -> Vec<Track> {
    let mut cache = TrackCache::load();
    let mut tracks = Vec::new();

    for dir in dirs {
        for entry in WalkDir::new(dir).follow_links(true).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }

            push_track_if_audio(entry.path(), &mut tracks, &mut cache);
        }
    }

    cache.clear_invalid();
    cache.save();

    tracks
}

pub fn scan_music_inputs(paths: &[PathBuf]) -> Vec<Track> {
    let mut cache = TrackCache::load();
    let mut tracks = Vec::new();

    for path in paths {
        if path.is_dir() {
            for entry in WalkDir::new(path).follow_links(true).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                push_track_if_audio(entry.path(), &mut tracks, &mut cache);
            }
        } else if path.is_file() {
            push_track_if_audio(path, &mut tracks, &mut cache);
        }
    }

    cache.clear_invalid();
    cache.save();

    tracks
}
