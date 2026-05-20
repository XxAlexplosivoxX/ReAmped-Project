use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub title: String,
    pub artist: String,
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
        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let current_time = duration.as_secs();
                    let current_size = metadata.len();
                    if let Some(cached) = self.get(path) {
                        return cached.modified_time == current_time && cached.file_size == current_size;
                    }
                }
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
