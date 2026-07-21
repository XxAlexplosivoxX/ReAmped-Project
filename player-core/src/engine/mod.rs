//! Engine module — audio playback loop, command dispatch, and state management.
//!
//! This module implements the core audio engine for ReAmped. It runs a dedicated
//! audio thread that owns the [`SymphoniaBackend`](crate::audio::symphonia_backend::SymphoniaBackend)
//! and processes [`command::PlayerCommand`]s sent from other threads. Playback state is
//! shared via an `Arc<Mutex<state::PlayerState>>`, and events are broadcast through an
//! [`event::EventBus`] for UI or other subscribers.
//!
//! ## Sub-modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`command`] | [`command::PlayerCommand`] enum — all operations the engine accepts |
//! | [`event`]   | [`event::Event`] enum and [`event::EventBus`] — observer-style notifications |
//! | [`state`]   | [`state::PlayerState`] — snapshot of current playback info |
//! | [`track`]   | [`track::Track`] — a single playable item |
//! | [`error`]   | [`error::PlayerError`] — typed errors emitted by the engine |
//! | [`player`]  | Audio loop, crossfade state machine, command handlers |

pub(crate) mod command;
pub(crate) mod state;
pub(crate) mod track;
pub(crate) mod player;
pub(crate) mod error;
pub(crate) mod event;
