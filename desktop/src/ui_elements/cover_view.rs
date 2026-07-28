use egui::{Color32, Sense};
use crate::PlayerApp;

const DEBUG_SWATCH_SIZE: f32 = 18.0;

pub fn show_cover(ui: &mut egui::Ui, player: &mut PlayerApp) {
    if let Some(texture) = &player.cover_texture {
        let response = ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(150.0, 150.0))
                .corner_radius(6.0)
                .sense(Sense::click()),
        );

        if response.clicked() {
            player.scroll_current_track = true;
        }
        if response.secondary_clicked() {
            player.show_palette_debug = !player.show_palette_debug;
        }
    }

    if player.show_palette_debug {
        let p = &player.palette;
        let roles = [
            ("primary", &p.primary),
            ("on_primary", &p.on_primary),
            ("primary_container", &p.primary_container),
            ("on_primary_container", &p.on_primary_container),
            ("secondary", &p.secondary),
            ("on_secondary", &p.on_secondary),
            ("secondary_container", &p.secondary_container),
            ("on_secondary_container", &p.on_secondary_container),
            ("tertiary", &p.tertiary),
            ("on_tertiary", &p.on_tertiary),
            ("tertiary_container", &p.tertiary_container),
            ("on_tertiary_container", &p.on_tertiary_container),
            ("error", &p.error),
            ("on_error", &p.on_error),
            ("error_container", &p.error_container),
            ("on_error_container", &p.on_error_container),
            ("surface", &p.surface),
            ("on_surface", &p.on_surface),
            ("surface_variant", &p.surface_variant),
            ("on_surface_variant", &p.on_surface_variant),
            ("outline", &p.outline),
            ("outline_variant", &p.outline_variant),
            ("background", &p.background),
            ("on_background", &p.on_background),
        ];

        egui::Window::new("Palette debug")
            .movable(true)
            .collapsible(false)
            .resizable(false)
            .default_size(egui::vec2(330.0, 400.0))
            .show(ui.ctx(), |ui| {
                egui::Grid::new("palette_grid")
                    .striped(true)
                    .min_col_width(100.0)
                    .show(ui, |ui| {
                        for (name, rgb) in &roles {
                            let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                            let text_color = if (rgb[0] as u16 + rgb[1] as u16 + rgb[2] as u16) / 3 > 128 {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            };
                            egui::Frame::new()
                                .fill(swatch)
                                .corner_radius(2.0)
                                .show(ui, |ui| {
                                    ui.set_min_size(egui::vec2(DEBUG_SWATCH_SIZE * 2.0, DEBUG_SWATCH_SIZE));
                                    ui.label(egui::RichText::new(*name).color(text_color));
                                });
                            ui.label(egui::RichText::new(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])).color(swatch));
                            ui.end_row();
                        }
                    });

                ui.separator();
                if ui.button("Cerrar").clicked() {
                    player.show_palette_debug = false;
                }
            });
    }
}
