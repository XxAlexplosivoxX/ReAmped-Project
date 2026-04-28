#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod player;
mod ui_elements;
mod utils;
mod dsp_ui;

use player_core::config::load_config;

use crate::{
    player::player_app_init::PlayerApp, utils::misc::setup_fonts,
};

fn main() -> eframe::Result<()> {
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
            let app = self::load_config();
            if app.fullscreen {
                cc.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(app.fullscreen));
            }
            Ok(Box::<PlayerApp>::default())
        }),
    )
}
