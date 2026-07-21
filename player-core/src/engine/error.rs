//! Error types emitted by the audio engine.

use std::fmt;

/// Errors that can occur during playback or command processing.
///
/// Each variant wraps a human-readable message string. Errors are published
/// through the `EventBus` as `Event::Error`
/// so that consumers can react without blocking the audio thread.
#[derive(Debug, Clone)]
pub enum PlayerError {
    /// A general playback failure (start, stop, resume, …).
    Playback(String),
    /// The audio file could not be decoded (unsupported format, corruption, …).
    Decode(String),
    /// An I/O error occurred while reading a file.
    Io(String),
    /// A command was invalid or could not be executed in the current state.
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
