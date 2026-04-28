use crate::player::player_app_init::PlayerApp;
use egui::{Color32, RichText, Sense, Stroke, Ui, Vec2};
use player_core::PlayerCommand;
use std::f32::consts::PI;

pub fn show_eq_controls(ui: &mut Ui, player: &mut PlayerApp, _accent: Color32, text: Color32) {
    // 1. Define exactly how wide this whole EQ block should be
    // 60-70 pixels is usually enough for 3 mini knobs
    ui.allocate_ui(Vec2::new(23.0, 120.0), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 3.0); // Tiny space between knobs
        ui.vertical_centered_justified(|ui| {
            egui::Frame::group(ui.style())
                .stroke(Stroke::new(1.0, text))
                .inner_margin(egui::Margin::same(0)) // Tight margin inside the border
                .show(ui, |ui| {
                    ui.add_space(7.5);
                    if draw_real_knob(ui, "H", &mut player.high_val, text).changed() {
                        player
                            .player
                            .send(PlayerCommand::SetGainHigh(player.high_val));
                    }
                    if draw_real_knob(ui, "M", &mut player.mid_val, text).changed() {
                        player
                            .player
                            .send(PlayerCommand::SetGainMid(player.mid_val));
                    }
                    if draw_real_knob(ui, "B", &mut player.bass_val, text).changed() {
                        player
                            .player
                            .send(PlayerCommand::SetGainBass(player.bass_val));
                    }
                    // ui.horizontal_centered(|ui| {
                    // });
                });
        });
    });
}

pub fn show_expander_knob(ui: &mut Ui, player: &mut PlayerApp, color: Color32) {
    if draw_real_knob(ui, "EX", &mut player.width_val, color).changed() {
        player
            .player
            .send(PlayerCommand::SetExpanderWidth(player.width_val));
    }
}

fn draw_real_knob(ui: &mut Ui, label: &str, value: &mut f32, accent: Color32) -> egui::Response {
    // Force a specific width for the knob + label column
    ui.allocate_ui(Vec2::new(20.0, 30.0), |ui| {
        ui.vertical_centered_justified(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0; // Tighten gap between knob and text

            // Center the knob manually
            let desired_size = Vec2::splat(15.0);
            let (rect, mut response) =
                ui.allocate_exact_size(desired_size, Sense::click_and_drag());

            if response.dragged() {
                let delta = response.drag_delta().y * 0.05;
                *value = (*value - delta).clamp(0.0, 2.0);
                response.mark_changed();
            }

            if response.double_clicked() {
                *value = 1.0;
                response.mark_changed();
            }


            if ui.is_rect_visible(rect) {
                let visuals = ui.style().interact(&response);
                let center = rect.center();
                let radius = rect.width() / 2.0;

                ui.painter().circle_filled(center, radius, visuals.bg_fill);
                ui.painter()
                    .circle_stroke(center, radius, visuals.fg_stroke);

                let start_angle = PI * 0.75;
                let end_angle = PI * 2.25;
                let angle = egui::lerp(start_angle..=end_angle, *value / 2.0);

                let line_start = center + Vec2::angled(angle) * (radius * 0.3);
                let line_end = center + Vec2::angled(angle) * (radius * 0.9);

                ui.painter().line_segment(
                    [line_start, line_end],
                    Stroke::new(1.5, accent), // Thinner needle for 15px knob
                );
            }

            // Draw the label centered under the knob
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.label(RichText::new(label).size(8.0).color(accent));
            });

            response.on_hover_ui_at_pointer(|ui| {
                ui.set_width(36.0);
                ui.label(RichText::new(format!("{:.2}", *value)));
            })
        })
        .inner
    })
    .inner
}
