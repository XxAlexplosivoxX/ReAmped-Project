use egui::Ui;
use crate::PlayerApp;

pub fn show_cover(ui: &mut Ui, player: &PlayerApp) {
    if let Some(texture) = &player.cover_texture {
        ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(150.0, 150.0))
                .corner_radius(6.0),
        );
    }
}
