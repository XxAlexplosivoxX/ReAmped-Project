//! Player UI module.
//!
//! This module contains the `PlayerApp` struct (the top-level egui
//! application state) and its [`eframe::App`] implementation that drives the
//! per-frame update loop.  Sub-modules are split by responsibility:
//!
//! * [`player_app_init`] — struct definition, constructor, and auxiliary helpers
//!   (cover loading, library scanning, media-sync background thread).
//! * [`update`] — the [`eframe::App::update`] method that paints the full UI.

pub mod player_app_init;
pub mod update;