//! Configurable keyboard-to-command mapping.
//!
//! Bindings are stored as `HashMap<String, String>` where keys are the
//! string representation of [`KeyCode`] variants and values are command
//! names (e.g. `"PlayPause"`, `"Next"`).  The mapping is serialised as
//! part of [`AppConfig`](crate::config::AppConfig).

use serde::{Serialize, Deserialize};
use crate::PlayerCommand;
use std::collections::HashMap;

/// A keyboard key that can be bound to a player action.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KeyCode {
    /// Space bar.
    Space,
    /// Enter / Return key.
    Enter,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// The `N` key.
    KeyN,
    /// The `P` key.
    KeyP,
    /// The `M` key.
    KeyM,
    /// The `R` key.
    KeyR,
    /// The `S` key.
    KeyS,
}

impl KeyCode {
    /// Returns the string representation used in the bindings map.
    pub fn to_string(&self) -> String {
        match self {
            KeyCode::Space => "Space".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::ArrowUp => "ArrowUp".to_string(),
            KeyCode::ArrowDown => "ArrowDown".to_string(),
            KeyCode::ArrowLeft => "ArrowLeft".to_string(),
            KeyCode::ArrowRight => "ArrowRight".to_string(),
            KeyCode::KeyN => "N".to_string(),
            KeyCode::KeyP => "P".to_string(),
            KeyCode::KeyM => "M".to_string(),
            KeyCode::KeyR => "R".to_string(),
            KeyCode::KeyS => "S".to_string(),
        }
    }
}

/// A mapping from keyboard keys to player commands.
///
/// The default bindings are:
///
/// | Key | Command |
/// |-----|---------|
/// | `Space`, `Enter` | `PlayPause` (toggle) |
/// | `ArrowRight` | `Next` |
/// | `ArrowLeft` | `Previous` |
/// | `M` | `Shuffle` |
/// | `R` | `Repeat` |
/// | `S` | `Stop` |
/// | `N` | `Play` |
/// | `P` | `Pause` |
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyBindings {
    /// Internal map of key string → command name.
    pub bindings: HashMap<String, String>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert("Space".to_string(), "PlayPause".to_string());
        bindings.insert("ArrowRight".to_string(), "Next".to_string());
        bindings.insert("ArrowLeft".to_string(), "Previous".to_string());
        bindings.insert("M".to_string(), "Shuffle".to_string());
        bindings.insert("R".to_string(), "Repeat".to_string());
        bindings.insert("S".to_string(), "Stop".to_string());
        bindings.insert("N".to_string(), "Play".to_string());
        bindings.insert("P".to_string(), "Pause".to_string());
        bindings.insert("Enter".to_string(), "PlayPause".to_string());

        Self { bindings }
    }
}

impl KeyBindings {
    /// Looks up the [`PlayerCommand`] bound to `key`.
    ///
    /// Returns `None` when the key is unbound or when the binding maps to
    /// the special `PlayPause` action (which must be handled separately via
    /// [`is_play_pause_key`](Self::is_play_pause_key)).
    pub fn get_command(&self, key: &KeyCode) -> Option<PlayerCommand> {
        let key_str = key.to_string();
        let command_str = self.bindings.get(&key_str)?;
        
        match command_str.as_str() {
            "PlayPause" => None, // Special case: needs to toggle
            "Play" => Some(PlayerCommand::Play),
            "Pause" => Some(PlayerCommand::Pause),
            "Next" => Some(PlayerCommand::Next),
            "Previous" => Some(PlayerCommand::Prev),
            "Stop" => Some(PlayerCommand::Stop),
            "Shuffle" => Some(PlayerCommand::AleatoryFullRandom),
            "Repeat" => Some(PlayerCommand::ToggleRepeat),
            "RepeatOne" => Some(PlayerCommand::ToggleRepeatOne),
            _ => None,
        }
    }

    /// Returns `true` when `key` is bound to the `PlayPause` toggle action.
    ///
    /// This is a separate check because `PlayPause` does not map directly to
    /// a [`PlayerCommand`] variant — the caller must determine the current
    /// playback state and send either `Play` or `Pause` accordingly.
    pub fn is_play_pause_key(&self, key: &KeyCode) -> bool {
        let key_str = key.to_string();
        self.bindings.get(&key_str)
            .map(|cmd| cmd == "PlayPause")
            .unwrap_or(false)
    }
}
