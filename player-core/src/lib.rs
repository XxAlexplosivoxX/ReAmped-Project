mod engine;
pub mod api;
pub mod audio;
pub mod dsp;
pub mod viz;
pub mod metadata;
pub mod config;
pub mod keybindings;

#[cfg(feature = "c-ffi")]
pub mod ffi;

pub use api::builder::PlayerBuilder;
pub use api::handle::Player;
pub use engine::command::PlayerCommand;
pub use engine::command::Options;
pub use engine::state::PlayerState;
pub use engine::track::Track;
pub use engine::event::Event;
pub use engine::error::PlayerError;
pub use keybindings::{KeyBindings, KeyCode};
