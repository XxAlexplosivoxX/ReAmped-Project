use std::fmt;

#[derive(Debug, Clone)]
pub enum PlayerError {
    Playback(String),
    Decode(String),
    Io(String),
    Command(String),
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayerError::Playback(msg) => write!(f, "Playback error: {}", msg),
            PlayerError::Decode(msg) => write!(f, "Decode error: {}", msg),
            PlayerError::Io(msg) => write!(f, "I/O error: {}", msg),
            PlayerError::Command(msg) => write!(f, "Command error: {}", msg),
        }
    }
}

impl std::error::Error for PlayerError {}
