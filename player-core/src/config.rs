//! Persistent application configuration stored as TOML.
//!
//! Configuration is loaded from the platform-specific config directory
//! (`~/.config/reamped/config.toml` on Linux).  When the file is missing or
//! corrupt, [`load_config`] falls back to [`AppConfig::default`].

use dirs_next::config_dir;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use crate::keybindings::KeyBindings;

/// Returns the platform-specific path to the config file.
///
/// The directory is created if it does not exist.  On Linux this resolves to
/// `$XDG_CONFIG_HOME/reamped/config.toml` (usually `~/.config/reamped/config.toml`).
fn config_path() -> PathBuf {
    let mut path = config_dir().expect("No config dir");
    path.push("reamped");
    std::fs::create_dir_all(&path).ok();
    path.push("config.toml");
    path
}

/// Top-level application configuration.
///
/// All fields are serialised to and deserialised from TOML via Serde.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    /// Master volume (`0.0` – `1.0`).
    pub volume: f32,
    /// Start the UI in fullscreen mode.
    pub fullscreen: bool,
    /// Enable crossfade between tracks.
    pub crossfade_enabled: bool,
    /// Crossfade duration in seconds.
    pub crossfade_seconds: f32,
    /// Automatically trim leading silence from tracks.
    #[serde(default)]
    pub silence_trim_enabled: bool,
    /// Colour theme settings.
    pub theme: ThemeConfig,
    /// FFT window size for the spectrum analyser.
    pub fft_size: usize,
    /// Apply smoothing to the spectrum display.
    pub spectrum_smooth: bool,
    /// Use line-mode rendering for the spectrum.
    pub line_mode: bool,
    /// Use the legacy rendering style.
    pub old_style: bool,
    /// Number of bars in the spectrum visualiser.
    pub spectrum_bars_quantity: usize,
    /// Directories scanned for music files.
    pub music_dirs: Vec<std::path::PathBuf>,
    /// User-customisable keyboard bindings.
    pub keybindings: KeyBindings,
}

/// Colour theme configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThemeConfig {
    /// Derive palette colours from the current cover art.
    pub follow_cover: bool,
    /// Base scale factor for the UI.
    pub base_scale: f32,
    /// Custom RGB palette entries (each entry is `[r, g, b]`).
    pub pallete_custom: Vec<[u8; 3]>,
}


impl Default for AppConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            fullscreen: false,
            crossfade_enabled: false,
            crossfade_seconds: 6.0,
            silence_trim_enabled: false,
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

/// Loads configuration from the platform config directory.
///
/// Returns [`AppConfig::default`] when the file cannot be read or parsed.
pub fn load_config() -> AppConfig {
    let path = config_path();

    if let Ok(data) = std::fs::read_to_string(&path) {
        toml::from_str(&data).unwrap_or_else(|_| AppConfig::default())
    } else {
        AppConfig::default()
    }
}

/// Persists the configuration to the platform config directory.
///
/// Silently ignores I/O errors (e.g. read-only filesystem).
pub fn save_config(cfg: &AppConfig) {
    let path = config_path();
    if let Ok(data) = toml::to_string_pretty(cfg) {
        let _ = std::fs::write(path, data);
    }
}

