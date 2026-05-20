use serde::{Serialize, Deserialize};
use crate::PlayerCommand;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KeyCode {
    Space,
    Enter,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    KeyN,
    KeyP,
    KeyM,
    KeyR,
    KeyS,
}

impl KeyCode {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyBindings {
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

    pub fn is_play_pause_key(&self, key: &KeyCode) -> bool {
        let key_str = key.to_string();
        self.bindings.get(&key_str)
            .map(|cmd| cmd == "PlayPause")
            .unwrap_or(false)
    }
}
