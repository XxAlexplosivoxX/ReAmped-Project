#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod player;
mod ui_elements;
mod utils;
mod dsp_ui;

use std::path::PathBuf;
use player_core::config::load_config;

use crate::{
    player::player_app_init::PlayerApp, utils::{misc::setup_fonts, scan_music_dirs::scan_music_inputs},
};

fn main() -> eframe::Result<()> {
    let startup_paths: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| !path.to_string_lossy().starts_with('-'))
        .collect();
    let startup_tracks = scan_music_inputs(&startup_paths);

    let options = eframe::NativeOptions {
        vsync: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 300.0])
            .with_resizable(false)
            .with_decorations(true),
        ..Default::default()
    };

    eframe::run_native(
        "ReAmped",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            let app = load_config();
            if app.fullscreen {
                cc.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(app.fullscreen));
            }
            Ok(Box::new(PlayerApp::new(startup_tracks.clone())))
        }),
    )
}
