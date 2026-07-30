#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Entry point for the ReAmped desktop application.
//!
//! This binary sets up an [`eframe`] native window and runs the [`PlayerApp`]
//! as the application state.  Any command-line arguments after the binary name
//! are treated as file or directory paths; audio files found there are loaded
//! into the playlist at startup.  The window is created at 550×300 logical
//! pixels, is not resizable, and honours the fullscreen setting from the
//! persisted configuration.

mod player;
mod ui_elements;
mod utils;
mod dsp_ui;

use std::path::PathBuf;
use player_core::config::load_config;

use crate::{
    player::player_app_init::PlayerApp, utils::{misc::setup_fonts, scan_music_dirs::scan_music_inputs},
};

/// Application entry point.
///
/// 1. Collects positional CLI arguments as startup paths.
/// 2. Scans those paths for supported audio files.
/// 3. Configures an [`eframe::NativeOptions`] with a fixed 550×300 window.
/// 4. Runs the native event loop, passing a freshly constructed [`PlayerApp`]
///    that already contains the startup tracks.
///
/// If the persisted configuration has `fullscreen = true`, the viewport is
/// switched to fullscreen before the first frame.
fn main() -> eframe::Result<()> {
    let config = load_config();

    let startup_paths: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| !path.to_string_lossy().starts_with('-'))
        .collect();
    let startup_tracks = scan_music_inputs(&startup_paths);

    let options = eframe::NativeOptions {
        vsync: config.vsync,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 310.0])
            .with_resizable(false)
            .with_decorations(true),
        ..Default::default()
    };

    let fullscreen = config.fullscreen;

    eframe::run_native(
        "ReAmped",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            if fullscreen {
                cc.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
            }
            Ok(Box::new(PlayerApp::new(startup_tracks.clone())))
        }),
    )
}
