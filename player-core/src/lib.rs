//! High-performance audio playback engine for the ReAmped music player.
//!
//! # Crate overview
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`api`] | Public API: [`Player`] handle and [`PlayerBuilder`] |
//! | [`audio`] | Audio rendering, ring buffer, visualization source |
//! | [`config`] | Persistent configuration (TOML) |
//! | [`dsp`] | Digital signal processing effects |
//! | `engine` | Internal playback thread, state machine, commands, events |
//! | `ffi` | C-compatible FFI bindings (behind `c-ffi` feature) |
//! | [`keybindings`] | Configurable keyboard mapping |
//! | [`metadata`] | Audio file tag reading and cover art |
//! | [`viz`] | Visualization data processing |
//!
//! # Feature flags
//!
//! - `c-ffi` — Exposes the `ffi` module with `extern "C"` functions for
//!   embedding the player in non-Rust hosts (C, C++, Python via ctypes, etc.).
//! - `symphonia-backend` — Use [Symphonia](https://github.com/pdeljanov/Symphonia)
//!   as the audio decoding backend instead of the default [lofty](https://github.com/Serial-ATA/lofty)-based
//!   path. Enable this for broader format support.
//!
//! # Threading model
//!
//! The engine runs on three cooperating threads:
//!
//! 1. **Audio thread** — Owns the decode-decouple-render pipeline. It reads
//!    audio files from disk, decodes PCM samples, pushes them into a shared
//!    ring buffer, and runs DSP effects. A CPAL audio callback reads from the
//!    ring buffer and writes to the hardware output.
//! 2. **Player thread** (spawned by [`PlayerBuilder::build`]) — Receives
//!    [`PlayerCommand`] values through an mpsc channel, drives the state
//!    machine, and publishes [`Event`] values to subscribers.
//! 3. **UI thread** — Any thread that holds a [`Player`] handle can send
//!    commands and poll events. The handle is [`Clone`] and [`Send`].
//!
//! # Safety guarantees
//!
//! - The CPAL audio callback is **lock-free** for ring-buffer reads. No mutex
//!   is acquired on the audio output path.
//! - Mutexes that protect [`PlayerState`] are **never held across the FFI
//!   boundary**. C callers that invoke `pc_player_get_state` receive a
//!   snapshot copy.
//! - All `extern "C"` functions perform null-pointer checks before dereferencing.

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
