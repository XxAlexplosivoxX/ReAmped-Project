use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub duration: f32,
    pub modified_time: u64,
    pub file_size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackCache {
    pub entries: HashMap<PathBuf, CacheEntry>,
}

impl TrackCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn load() -> Self {
        let cache_path = Self::cache_file_path();

        match fs::read_to_string(&cache_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(cache) => cache,
                Err(_) => Self::new(),
            },
            Err(_) => Self::new(),
        }
    }

    pub fn save(&self) {
        let cache_path = Self::cache_file_path();

        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&cache_path, json);
        }
    }

    fn cache_file_path() -> PathBuf {
        if let Some(config_dir) = dirs_next::config_dir() {
            config_dir.join("ReAmped").join("track_cache.json")
        } else {
            PathBuf::from(".track_cache.json")
        }
    }

    pub fn get(&self, path: &Path) -> Option<CacheEntry> {
        self.entries.get(path).cloned()
    }

    pub fn is_valid(&self, path: &Path) -> bool {
        if let Ok(metadata) = fs::metadata(path)
            && let Ok(modified) = metadata.modified()
            && let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH)
        {
            let current_time = duration.as_secs();
            let current_size = metadata.len();
            if let Some(cached) = self.get(path) {
                return cached.modified_time == current_time && cached.file_size == current_size;
            }
        }
        false
    }

    pub fn insert(&mut self, path: PathBuf, entry: CacheEntry) {
        self.entries.insert(path, entry);
    }

    pub fn clear_invalid(&mut self) {
        self.entries.retain(|path, _| path.exists());
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheEntry, TrackCache};
    use std::path::Path;

    #[test]
    fn deserialize_cache_without_album() {
        let cache: TrackCache = serde_json::from_str(
            r#"{"entries":{"song.flac":{"title":"Song","artist":"Artist","duration":120.0,"modified_time":123,"file_size":456}}}"#,
        ).unwrap();
        let entry = cache.get(Path::new("song.flac")).unwrap();

        assert_eq!(entry.album, "");
        assert_eq!(entry.title, "Song");
        assert_eq!(entry.artist, "Artist");
        assert_eq!(entry.duration, 120.0);
        assert_eq!(entry.modified_time, 123);
        assert_eq!(entry.file_size, 456);
    }

    #[test]
    fn album_survives_cache_serialization() {
        for album in ["", "Álbum"] {
            let mut cache = TrackCache::new();
            cache.insert(
                "song.flac".into(),
                CacheEntry {
                    title: "Song".into(),
                    artist: "Artist".into(),
                    album: album.into(),
                    duration: 120.0,
                    modified_time: 123,
                    file_size: 456,
                },
            );
            let json = serde_json::to_string(&cache).unwrap();
            let restored: TrackCache = serde_json::from_str(&json).unwrap();
            let entry = restored.get(Path::new("song.flac")).unwrap();

            assert_eq!(entry.album, album);
            assert_eq!(entry.modified_time, 123);
            assert_eq!(entry.file_size, 456);
        }
    }
}
