use std::path::PathBuf;
use serde::{Serialize, Deserialize};

#[derive(PartialEq, PartialOrd, Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub duration: f32,
}
