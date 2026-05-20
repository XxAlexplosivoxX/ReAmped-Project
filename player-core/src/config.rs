use dirs_next::config_dir;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use crate::keybindings::KeyBindings;

fn config_path() -> PathBuf {
    let mut path = config_dir().expect("No config dir");
    path.push("reamped");
    std::fs::create_dir_all(&path).ok();
    path.push("config.toml");
    path
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub volume: f32,
    pub fullscreen: bool,
    pub crossfade_seconds: f32,
    pub theme: ThemeConfig,
    pub fft_size: usize,
    pub spectrum_smooth: bool,
    pub line_mode: bool,
    pub old_style: bool,
    pub spectrum_bars_quantity: usize,
    pub music_dirs: Vec<std::path::PathBuf>,
    pub keybindings: KeyBindings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThemeConfig {
    pub follow_cover: bool,
    pub base_scale: f32,
    pub pallete_custom: Vec<[u8; 3]>,
}


impl Default for AppConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            fullscreen: false,
            crossfade_seconds: 20.0,
            theme: ThemeConfig {
                follow_cover: true,
                base_scale: 1.0,
                pallete_custom: vec![[36, 36, 36], [209, 209, 209], [140, 140, 140]],
            },
            fft_size: 13000,
            line_mode: false,
            old_style: false,
            spectrum_bars_quantity: 300,
            spectrum_smooth: false,
            music_dirs: Vec::new(),
            keybindings: KeyBindings::default(),
        }
    }
}

pub fn load_config() -> AppConfig {
    let path = config_path();

    if let Ok(data) = std::fs::read_to_string(&path) {
        toml::from_str(&data).unwrap_or_else(|_| AppConfig::default())
    } else {
        AppConfig::default()
    }
}

pub fn save_config(cfg: &AppConfig) {
    let path = config_path();
    if let Ok(data) = toml::to_string_pretty(cfg) {
        let _ = std::fs::write(path, data);
    }
}

