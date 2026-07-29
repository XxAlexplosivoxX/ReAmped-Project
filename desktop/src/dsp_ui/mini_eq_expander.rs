use crate::player::player_app_init::PlayerApp;
use egui::{Color32, RichText, Sense, Stroke, Ui, Vec2};
use player_core::{PlayerCommand, config::save_config};
use std::f32::consts::PI;

pub fn show_eq_controls(ui: &mut Ui, player: &mut PlayerApp, _accent: Color32, text: Color32) {
    let on_p = Color32::from_rgb(
        player.palette.on_primary[0],
        player.palette.on_primary[1],
        player.palette.on_primary[2],
    );
    ui.allocate_ui(Vec2::new(23.0, 120.0), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 3.0);
        ui.vertical_centered_justified(|ui| {
            egui::Frame::group(ui.style())
                .stroke(Stroke::new(1.0, text))
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.add_space(7.5);
                    let save = |p: &mut PlayerApp| {
                        if let Ok(mut cfg) = p.config.lock() {
                            cfg.high_val = p.high_val;
                            cfg.mid_val = p.mid_val;
                            cfg.bass_val = p.bass_val;
                            cfg.width_val = p.width_val;
                            save_config(&cfg);
                        }
                    };
                    if draw_real_knob(ui, "H", &mut player.high_val, text, on_p).changed() {
                        player
                            .player
                            .send(PlayerCommand::SetGainHigh(player.high_val));
                        save(player);
                    }
                    if draw_real_knob(ui, "M", &mut player.mid_val, text, on_p).changed() {
                        player
                            .player
                            .send(PlayerCommand::SetGainMid(player.mid_val));
                        save(player);
                    }
                    if draw_real_knob(ui, "B", &mut player.bass_val, text, on_p).changed() {
                        player
                            .player
                            .send(PlayerCommand::SetGainBass(player.bass_val));
                        save(player);
                    }
                });
        });
    });
}

pub fn show_expander_knob(ui: &mut Ui, player: &mut PlayerApp, color: Color32) {
    let on_p = Color32::from_rgb(
        player.palette.on_primary[0],
        player.palette.on_primary[1],
        player.palette.on_primary[2],
    );
    if draw_real_knob(ui, "EX", &mut player.width_val, color, on_p).changed() {
        player
            .player
            .send(PlayerCommand::SetExpanderWidth(player.width_val));
        if let Ok(mut cfg) = player.config.lock() {
            cfg.width_val = player.width_val;
            save_config(&cfg);
        }
    }
}

fn draw_real_knob(ui: &mut Ui, label: &str, value: &mut f32, accent: Color32, on_primary: Color32) -> egui::Response {
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

            if response.has_focus() || response.hovered() {
                let mut changed = false;
                let mut handle_key = |key: egui::Key, delta: f32| {
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                        *value = (*value + delta).clamp(0.0, 2.0);
                        changed = true;
                    }
                };
                handle_key(egui::Key::ArrowUp, 0.1);
                handle_key(egui::Key::ArrowDown, -0.1);
                if changed {
                    response.mark_changed();
                }
            }


            if ui.is_rect_visible(rect) {
                let center = rect.center();
                let radius = rect.width() / 2.0;

                let fill = if response.has_focus() || response.hovered() {
                    on_primary.linear_multiply(1.3)
                } else {
                    on_primary
                };
                ui.painter().circle_filled(center, radius, fill);
                ui.painter().circle_stroke(center, radius, egui::Stroke::new(1.0, accent));

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
