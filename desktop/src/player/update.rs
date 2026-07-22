//! Per‑frame update loop (`eframe::App::update`).
//!
//! # Layout overview
//!
//! The [`update`](eframe::App::update) method paints everything inside an
//! [`egui::CentralPanel`] whose background is a slanted vertical gradient
//! (see [`draw_slanted_vertical_gradient`]).  Sub‑components are called as
//! plain functions — there are no nested widget structs.
//!
//! The layout is roughly:
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  Album art  │  Buttons & title              │
//! │  (cover)    │  ┌──────────┬──────────────┐  │
//! │             │  │ Volume   │ Order btns   │  │
//! │             │  │ bar      │ & expander   │  │
//! │             │  ├──────────┴──────────────┤  │
//! │             │  │ VU meters │ Search +    │  │
//! │             │  │           │ miniplaylist│  │
//! │             │  │           │ Spectrum    │  │
//! │             │  └──────────┴──────────────┘  │
//! ├─────────────────────────────────────────────┤
//! │             Seek slider                     │
//! ├─────────────────────────────────────────────┤
//! │         Waveform / beat stripes             │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # Repaint scheduling
//!
//! Every frame calls
//! `ctx.request_repaint_after(Duration::from_millis(16))` — both when playing
//! and when paused — to keep the UI updating at roughly 60 fps.  The waveform,
//! spectrum, and position slider all rely on this steady tick.
//!
//! # Data flow
//!
//! * **Player state** — `self.player.position()`, `.is_playing()`,
//!   `.duration()`, `.samples()`, `.get_loudness()` are polled every frame.
//! * **Shared samples** — `self.player.samples()` returns an `Arc` behind
//!   which lives a lock‑free ring buffer written by the audio thread and read
//!   by the UI thread.  Both the spectrum visualiser and the waveform
//!   visualiser consume this buffer.
//! * **Keyboard input** — [`handle_keyboard_input`] reads the current egui
//!   input state and translates it to a [`PlayerCommand`] according to the
//!   user's configured keybindings.
//! * **Library loading** — if the playlist is empty,
//!   [`PlayerApp::load_library_async`] is
//!   called to kick off a background scan.

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
    /// Called by eframe every time the window needs repainting.
    ///
    /// This method:
    /// 1. Adjusts [`pixels_per_point`](egui::Context::set_pixels_per_point) so
    ///    that the UI scales with the physical viewport width.
    /// 2. Calls [`ensure_cover_loaded`](PlayerApp::ensure_cover_loaded) to
    ///    update the colour theme if the track changed.
    /// 3. Draws the slanted‑gradient background via
    ///    [`draw_slanted_vertical_gradient`].
    /// 4. Lays out all sub‑components (cover, buttons, volume bar, order
    ///    buttons, EQ, VU meters, search + mini‑playlist, spectrum visualiser).
    /// 5. Renders a seek slider that polls the core position and sends
    ///    [`PlayerCommand::Seek`] on drag.
    /// 6. Draws the synchronised waveform and beat stripes below the slider.
    /// 7. Requests another repaint after 16 ms (~60 fps).
    /// 8. If the playlist is empty, kicks off a background library scan.
    /// 9. Handles keyboard shortcuts via [`handle_keyboard_input`].
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let physical_width = ctx.input(|i| i.viewport_rect().width() * i.pixels_per_point());

        let base_width = 532.0;

        let target_scale = physical_width / base_width;

        ctx.set_pixels_per_point(target_scale);

        let dt = ctx.input(|i| i.unstable_dt);
        self.bg_anim_t += dt;

        for i in 0..3 {
            let c = &self.target_palette_sorted[i];
            let cur = &self.palette_sorted[i];
            self.palette_sorted[i] = [
                lerp_u8(cur[0], c[0], 0.08),
                lerp_u8(cur[1], c[1], 0.08),
                lerp_u8(cur[2], c[2], 0.08),
            ];
        }
        let palette = self.palette_sorted.clone();
        let panel =
            Color32::from_rgba_unmultiplied_const(palette[2][0], palette[2][1], palette[2][2], 120);
        let accent = panel.clone();
        let accent = accent.gamma_multiply(1.2);
        let text = Color32::from_rgb(palette[0][0], palette[0][1], palette[0][2]);

        let anim = self.bg_anim_t * 0.04;
        let shift = |v: u8| -> u8 {
            let s = (anim.sin() * 20.0) as i32;
            (v as i32 + s).clamp(0, 255) as u8
        };

        self.ensure_cover_loaded(&ctx, false);
        show_config_window(self, ctx, accent);

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter();
            draw_slanted_vertical_gradient(
                painter,
                rect,
                Color32::from_rgb(
                    shift(palette[2][0]),
                    shift(palette[2][1]),
                    shift(palette[2][2]),
                ),
                Color32::from_rgb(
                    shift(palette[1][0]),
                    shift(palette[1][1]),
                    shift(palette[1][2]),
                ),
                -6.0,
            );
            ui.horizontal(|ui| {
                show_cover(ui, self);
                ui.vertical(|ui| {
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
                                    let samples = self.player.samples();
                                    let palette = &self.palette_sorted;

                                    self.visualizer.draw_spectrum(
                                        ui,
                                        samples,
                                        palette,
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
            );
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
                self.state = "status: Playing";
                self.just_executed = false;
            } else {
                self.state = "status: Paused"
            }

            ctx.request_repaint_after(Duration::from_millis(16));

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

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}
