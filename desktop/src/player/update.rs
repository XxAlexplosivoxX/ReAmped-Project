use egui::{Color32, style::HandleShape};
use player_core::{PlayerCommand, viz::waveform::synchronized_waveform};
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
        let physical_width = ctx.input(|i| i.viewport_rect().width() * i.pixels_per_point());

        let base_width = 532.0;

        let target_scale = physical_width / base_width;

        ctx.set_pixels_per_point(target_scale);

        let dt = ctx.input(|i| i.unstable_dt);
        self.bg_anim_t += dt;

        self.palette = self.palette.lerp(&self.target_palette, 0.08);

        let current_path = self.current_track().map(|t| t.path);
        if current_path != self.last_scrolled_track {
            self.last_scrolled_track = current_path;
            self.scroll_current_track = true;
        }

        let _surface_rgb = self.palette.surface;
        let on_surface_rgb = self.palette.on_surface;
        let primary_rgb = self.palette.primary;
        let secondary_rgb = self.palette.on_secondary;
        let on_primary = self.palette.on_primary;
        let on_primary_cont = self.palette.on_primary_container;
        
        let accent = Color32::from_rgba_unmultiplied_const(primary_rgb[0], primary_rgb[1], primary_rgb[2], 100);
        let text = Color32::from_rgb(on_surface_rgb[0], on_surface_rgb[1], on_surface_rgb[2]);

        let anim = self.bg_anim_t * 0.3;
        let shift = |v: u8| -> u8 {
            let s = (anim.sin() * 20.0) as i32;
            (v as i32 + s).clamp(0, 255) as u8
        };

        let bg_bot = Color32::from_rgb(
            shift(on_primary_cont[0]),
            shift(on_primary_cont[1]),
            shift(on_primary_cont[2]),
        ).gamma_multiply(0.8);
        let bg_top = Color32::from_rgb(secondary_rgb[0], secondary_rgb[1], secondary_rgb[2]).gamma_multiply(0.8);
        let wave_col = Color32::from_rgb(primary_rgb[0], primary_rgb[1], primary_rgb[2]).gamma_multiply(0.7);

        self.apply_pending_cover(&ctx);
        self.ensure_cover_loaded(&ctx, false);
        show_config_window(self, ctx, accent);

        {
            let cfg = self.config.lock().unwrap();
            let fps = cfg.target_fps.max(1).min(240);
            let interval = Duration::from_millis((1000 / fps) as u64);
            ctx.request_repaint_after(interval);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter();
            draw_slanted_vertical_gradient(painter, rect, bg_top, bg_bot, -12.0);
            ui.horizontal(|ui| {
                show_cover(ui, self);
                ui.vertical(|ui| {
                    show_buttons_and_title(ui, ctx, self, text);
                    ui.horizontal(|ui| {
                        show_eq_controls(ui, self, accent, text);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                show_volume_bar(ui, self);
                                show_order_buttons(ui, self);
                                ui.vertical(|ui|{
                                    ui.add_space(-6.0);
                                    show_expander_knob(ui, self, text);
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
                                    show_search_and_miniplaylist(ui, self);
                                    let samples = self.player.samples();
                                    self.visualizer.draw_spectrum(
                                        ui,
                                        samples,
                                        &self.palette,
                                    );
                                });
                            });
                        });
                    });
                });
            });
            ui.horizontal(|ui| {
                let mut duration = self.player.duration();

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
                let on_primary_col = Color32::from_rgb(on_primary[0], on_primary[1], on_primary[2]);
                let prev_inactive = ui.style().visuals.widgets.inactive.bg_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = on_primary_col;
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
                ui.style_mut().visuals.widgets.inactive.bg_fill = prev_inactive;
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
                    self.state = "status: Seeking"
                }
                if response.drag_stopped() {
                    self.player.send(PlayerCommand::Seek(pos));
                    self.state = "status: Playing";
                }
            });
            let height = ui.available_height() - 10.0;

            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::hover(),
            );

            let painter = ui.painter_at(rect).with_clip_rect(rect);
            let samples = self.player.samples().clone();
            let wave = synchronized_waveform(
                samples,
                8192,
                self.player.get_sample_rate(),
                &mut self.visualizer.last_period,
                &mut self.visualizer.last_wave_window,
            );

            draw_waveform_raw(&painter, rect, &wave, wave_col, Color32::from_black_alpha(25));

            let on_primary_color = Color32::from_rgb(on_primary[0], on_primary[1], on_primary[2]);
            self.visualizer.draw_beat_stripes(ui, accent, on_primary_color);

            if self.player.is_playing() {
                self.state = "status: Playing";
                self.just_executed = false;
            } else {
                self.state = "status: Paused"
            }

            if self.player.playlist().is_empty() {
                self.load_library_async();
            } else if self.library_loading {
                self.library_loading = false;
            }

            let config = self.config.lock().unwrap();
            let keybindings = config.keybindings.clone();
            drop(config);

            if let Some(cmd) = handle_keyboard_input(ctx, &keybindings, self.player.is_playing()) {
                self.player.send(cmd);
            }
        });
    }
}
