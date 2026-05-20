use egui::{Color32, style::HandleShape};
use player_core::{PlayerCommand, viz::waveform::waveform};
use std::time::Duration;

use crate::{
    dsp_ui::{
        db_meter::draw_vertical_meter,
        mini_eq_expander::{show_eq_controls, show_expander_knob},
    },
    player::player_app_init::PlayerApp,
    ui_elements::{
        buttons::show_buttons_and_title, config_window::show_config_window, cover_view::show_cover,
        order_buttons::show_order_buttons, search_and_miniplaylist::show_search_and_miniplaylist,
        volume_bar::show_volume_bar,
    },
    utils::{background::draw_slanted_vertical_gradient, visualizer::draw_waveform_raw, keyboard::handle_keyboard_input},
};

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let palette = self.palette_sorted.clone();
        let panel =
            Color32::from_rgba_unmultiplied_const(palette[2][0], palette[2][1], palette[2][2], 120);
        let accent = panel.clone();
        let accent = accent.gamma_multiply(1.2);
        let text = Color32::from_rgb(palette[0][0], palette[0][1], palette[0][2]);
        let physical_width = ctx.input(|i| i.viewport_rect().width() * i.pixels_per_point());

        let base_width = 532.0;

        let target_scale = physical_width / base_width;

        ctx.set_pixels_per_point(target_scale);

        self.ensure_cover_loaded(&ctx, false);
        show_config_window(self, ctx, accent);

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter();
            draw_slanted_vertical_gradient(
                painter,
                rect,
                Color32::from_rgb(palette[2][0], palette[2][1], palette[2][2]),
                Color32::from_rgb(palette[1][0], palette[1][1], palette[1][2]),
                -6.0,
            );
            ui.horizontal(|ui| {
                show_cover(ui, self);
                ui.vertical(|ui| {
                    // ui.horizontal(|ui| {
                    //     ui.vertical(|ui| {
                    //         ui.horizontal(|ui| {
                    //             let plugins = self.player.plugins_info();
                    //             let plugins = plugins.lock().unwrap();
                    //             let value1 = plugins.get_key_value("VU Meter");
                    //             let value2 = plugins.get_key_value("RMS Meter");
                    //             ui.vertical(|ui| {
                    //                 if value1.is_some() {
                    //                     draw_meter(ui, value1.unwrap().1.clone(), accent, text);
                    //                     ui.label(format!("{:.1}", *value1.unwrap().1));
                    //                 } else {
                    //                     draw_meter(ui, 0.0, accent, text);
                    //                     ui.label(format!("{:.1}", 0.0));
                    //                 }
                    //             });
                    //             ui.vertical(|ui| {
                    //                 if value2.is_some() {
                    //                     draw_meter(ui, value2.unwrap().1.clone(), accent, text);
                    //                     ui.label(format!("{:.1}", *value2.unwrap().1));
                    //                 } else {
                    //                     draw_meter(ui, 0.0, accent, text);
                    //                     ui.label(format!("{:.1}", 0.0));
                    //                 }
                    //             });
                    //         });
                    //     });
                    // });
                    show_buttons_and_title(ui, ctx, self, self.text_color.clone(), accent);
                    ui.horizontal(|ui| {
                        // 1. EQ on the far left
                        show_eq_controls(ui, self, accent, self.text_color);
                        // 2. Everything else in a vertical column to the right
                        ui.vertical(|ui| {
                            // Top row: Volume and Order
                            ui.horizontal(|ui| {
                                show_volume_bar(ui, self);
                                show_order_buttons(ui, self, accent, self.text_color);
                                ui.vertical(|ui|{
                                    ui.add_space(-6.0);
                                    show_expander_knob(ui, self, self.text_color);
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.horizontal(|ui| {
                                    let (ldness_l, ldness_r) = self.player.get_loudness();
                                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                                    draw_vertical_meter(ui, ldness_r);
                                    draw_vertical_meter(ui, ldness_l);
                                });
                                ui.vertical(|ui| {
                                    show_search_and_miniplaylist(ui, self, accent);
                                    let samples = &self.player.samples;
                                    let palette = &self.palette_sorted;

                                    self.visualizer.draw_spectrum(
                                        ui,
                                        samples,
                                        palette[0][0],
                                        palette[0][1],
                                        palette[0][2],
                                    );
                                });
                            });
                        });
                    });
                });
            });
            ui.horizontal(|ui| {
                let mut duration = self.player.state.lock().unwrap().duration;

                let available = ui.available_width() - 76.0 - 16.0;

                let has_track = duration > 0.0;
                let mut pos = self.position;
                if !has_track {
                    pos = 0.0;
                    duration = 0.01;
                }
                ui.add_sized(
                    [38.0, 20.5],
                    egui::Label::new(format!(
                        "{:02}:{:02}",
                        pos.clone() as u32 / 60,
                        pos.clone() as u32 % 60
                    )),
                );
                ui.style_mut().spacing.slider_width = available;
                let response = ui.add_enabled(
                    has_track,
                    egui::Slider::new(&mut pos, 0.0..=duration)
                        .show_value(false)
                        .step_by(0.1)
                        .handle_shape(HandleShape::Rect {
                            aspect_ratio: (1.0),
                        })
                        .trailing_fill(true),
                );
                ui.add_sized(
                    [38.0, 20.5],
                    egui::Label::new(format!(
                        "{:02}:{:02}",
                        duration.clone() as u32 / 60,
                        duration.clone() as u32 % 60
                    )),
                );

                if has_track && !response.dragged() {
                    self.position = self.player.position();
                }
                if response.dragged() {
                    self.state = String::from("status: Seeking")
                }
                if response.drag_stopped() {
                    self.player.send(PlayerCommand::Seek(pos));
                    self.state = String::from("status: Playing");
                }
            });
            let height = ui.available_height() - 10.0;

            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::hover(),
            );

            let painter = ui.painter_at(rect).with_clip_rect(rect);
            let samples = self.player.samples.clone();
            let wave = waveform(samples, 4108);
            let palette = &self.palette_sorted;

            draw_waveform_raw(
                &painter,
                rect,
                &wave,
                Color32::from_rgb(palette[0][0], palette[0][1], palette[0][2]).gamma_multiply(0.6),
                Color32::TRANSPARENT,
            );
            self.visualizer.draw_beat_stripes(ui, accent, text);
            if self.player.is_playing() {
                self.state = String::from("status: Playing");
                self.just_executed = false;
            } else {
                self.state = String::from("status: Paused")
            }

            if self.player.is_playing() {
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }

            if self.player.playlist().is_empty() {
                self.load_library_async();
            }

            // Handle keyboard input
            let config = self.config.lock().unwrap();
            let keybindings = config.keybindings.clone();
            drop(config);
            
            if let Some(cmd) = handle_keyboard_input(ctx, &keybindings, self.player.is_playing()) {
                self.player.send(cmd);
            }
        });
    }
}
