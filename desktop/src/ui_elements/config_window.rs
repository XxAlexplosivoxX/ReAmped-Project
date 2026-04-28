use egui::{Color32, Context};
use player_core::config::save_config;

use crate::{ui_elements::music_dirs::draw_music_dirs, player::player_app_init::PlayerApp};


pub fn show_config_window(player: &mut PlayerApp, ctx: &Context, accent: Color32) {
    if player.show_settings {
        let mut show_settings = player.show_settings;

        egui::Window::new("Configuración")
            .movable(true)
            .collapsible(false)
            .resizable(false)
            .open(&mut show_settings)
            .frame({
                let mut frame = egui::Frame::window(&ctx.style());

                frame.fill = accent.linear_multiply(1.2);
                frame.fill = egui::Color32::from_rgba_unmultiplied(
                    frame.fill.r(),
                    frame.fill.g(),
                    frame.fill.b(),
                    210,
                );

                frame
            })
            .show(ctx, |ui| {
                egui::scroll_area::ScrollArea::vertical()
                    .max_height(ui.available_height() - 20.0)
                    .show(ui, |ui| {
                        let style = ui.style_mut();
                        style.animation_time = 2.0;

                        let mut reload_cover = false;
                        let mut reload_library = false;

                        {
                            let mut cfg = player.config.lock().unwrap();

                            ui.heading("General");
                            ui.separator();

                            if ui
                                .checkbox(
                                    &mut cfg.fullscreen,
                                    "Abrir en pantalla completa por default",
                                )
                                .changed()
                            {
                                save_config(&cfg);
                            }

                            ui.add_space(10.0);
                            ui.heading("Configuración del tema");
                            ui.separator();

                            if ui
                                .checkbox(&mut cfg.theme.follow_cover, "cambiar colores por cover")
                                .changed()
                            {
                                save_config(&cfg);
                                reload_cover = true;
                            }

                            if !cfg.theme.follow_cover {
                                ui.label("primer color de la paleta");

                                let color = egui::Color32::from_rgb(
                                    (player.rgb1[0] * 255.0) as u8,
                                    (player.rgb1[1] * 255.0) as u8,
                                    (player.rgb1[2] * 255.0) as u8,
                                );

                                if ui.colored_label(color, "██████").clicked() {
                                    player.show_picker1 = true;
                                }

                                ui.label("segundo color de la paleta");

                                let color = egui::Color32::from_rgb(
                                    (player.rgb2[0] * 255.0) as u8,
                                    (player.rgb2[1] * 255.0) as u8,
                                    (player.rgb2[2] * 255.0) as u8,
                                );

                                if ui.colored_label(color, "██████").clicked() {
                                    player.show_picker2 = true;
                                }

                                ui.label("tercer color de la paleta");

                                let color = egui::Color32::from_rgb(
                                    (player.rgb3[0] * 255.0) as u8,
                                    (player.rgb3[1] * 255.0) as u8,
                                    (player.rgb3[2] * 255.0) as u8,
                                );

                                if ui.colored_label(color, "██████").clicked() {
                                    player.show_picker3 = true;
                                }

                                let mut open_picker1 = player.show_picker1;
                                if open_picker1 {
                                    egui::Window::new("")
                                        .title_bar(false)
                                        .resizable(false)
                                        .collapsible(false)
                                        .fixed_size(egui::vec2(200.0, 160.0))
                                        .movable(true)
                                        .frame({
                                            let mut frame = egui::Frame::window(&ctx.style());

                                            frame.fill = accent.linear_multiply(1.2);
                                            frame.fill = egui::Color32::from_rgba_unmultiplied(
                                                frame.fill.r(),
                                                frame.fill.g(),
                                                frame.fill.b(),
                                                210,
                                            );

                                            frame
                                        })
                                        .show(ctx, |ui| {
                                            ui.vertical_centered(|ui| {
                                                ui.heading("Color 1");

                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb1[0], 0.0..=1.0)
                                                        .text("R"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb1[1], 0.0..=1.0)
                                                        .text("G"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb1[2], 0.0..=1.0)
                                                        .text("B"),
                                                );

                                                ui.add_space(6.0);

                                                let color = egui::Color32::from_rgb(
                                                    (player.rgb1[0] * 255.0) as u8,
                                                    (player.rgb1[1] * 255.0) as u8,
                                                    (player.rgb1[2] * 255.0) as u8,
                                                );

                                                ui.horizontal(|ui| {
                                                    ui.label("Preview");
                                                    ui.colored_label(color, "██████");
                                                });

                                                ui.add_space(8.0);

                                                if ui.button("OK").clicked() {
                                                    cfg.theme.pallete_custom[0] = [
                                                        (player.rgb1[0] * 255.0) as u8,
                                                        (player.rgb1[1] * 255.0) as u8,
                                                        (player.rgb1[2] * 255.0) as u8,
                                                    ];
                                                    save_config(&cfg);
                                                    open_picker1 = false;
                                                }
                                            });
                                        });
                                }
                                player.show_picker1 = open_picker1;

                                let mut open_picker2 = player.show_picker2;
                                if open_picker2 {
                                    egui::Window::new("")
                                        .title_bar(false)
                                        .resizable(false)
                                        .collapsible(false)
                                        .movable(true)
                                        .fixed_size(egui::vec2(200.0, 160.0))
                                        .frame({
                                            let mut frame = egui::Frame::window(&ctx.style());

                                            frame.fill = accent.linear_multiply(1.2);
                                            frame.fill = egui::Color32::from_rgba_unmultiplied(
                                                frame.fill.r(),
                                                frame.fill.g(),
                                                frame.fill.b(),
                                                210,
                                            );

                                            frame
                                        })
                                        .show(ctx, |ui| {
                                            ui.vertical_centered(|ui| {
                                                ui.heading("Color 2");

                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb2[0], 0.0..=1.0)
                                                        .text("R"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb2[1], 0.0..=1.0)
                                                        .text("G"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb2[2], 0.0..=1.0)
                                                        .text("B"),
                                                );

                                                ui.add_space(6.0);

                                                let color = egui::Color32::from_rgb(
                                                    (player.rgb2[0] * 255.0) as u8,
                                                    (player.rgb2[1] * 255.0) as u8,
                                                    (player.rgb2[2] * 255.0) as u8,
                                                );

                                                ui.horizontal(|ui| {
                                                    ui.label("Preview");
                                                    ui.colored_label(color, "██████");
                                                });

                                                ui.add_space(8.0);

                                                if ui.button("OK").clicked() {
                                                    cfg.theme.pallete_custom[1] = [
                                                        (player.rgb2[0] * 255.0) as u8,
                                                        (player.rgb2[1] * 255.0) as u8,
                                                        (player.rgb2[2] * 255.0) as u8,
                                                    ];
                                                    save_config(&cfg);
                                                    open_picker2 = false;
                                                }
                                            });
                                        });
                                }
                                player.show_picker2 = open_picker2;

                                let mut open_picker3 = player.show_picker3;
                                if open_picker3 {
                                    egui::Window::new("")
                                        .title_bar(false)
                                        .resizable(false)
                                        .collapsible(false)
                                        .fixed_size(egui::vec2(200.0, 160.0))
                                        .movable(true)
                                        .frame({
                                            let mut frame = egui::Frame::window(&ctx.style());

                                            frame.fill = accent.linear_multiply(1.2);
                                            frame.fill = egui::Color32::from_rgba_unmultiplied(
                                                frame.fill.r(),
                                                frame.fill.g(),
                                                frame.fill.b(),
                                                210,
                                            );

                                            frame
                                        })
                                        .show(ctx, |ui| {
                                            ui.vertical_centered(|ui| {
                                                ui.heading("Color 3");

                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb3[0], 0.0..=1.0)
                                                        .text("R"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb3[1], 0.0..=1.0)
                                                        .text("G"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(&mut player.rgb3[2], 0.0..=1.0)
                                                        .text("B"),
                                                );

                                                ui.add_space(6.0);

                                                let color = egui::Color32::from_rgb(
                                                    (player.rgb3[0] * 255.0) as u8,
                                                    (player.rgb3[1] * 255.0) as u8,
                                                    (player.rgb3[2] * 255.0) as u8,
                                                );

                                                ui.horizontal(|ui| {
                                                    ui.label("Preview");
                                                    ui.colored_label(color, "██████");
                                                });

                                                ui.add_space(8.0);

                                                if ui.button("OK").clicked() {
                                                    cfg.theme.pallete_custom[2] = [
                                                        (player.rgb3[0] * 255.0) as u8,
                                                        (player.rgb3[1] * 255.0) as u8,
                                                        (player.rgb3[2] * 255.0) as u8,
                                                    ];
                                                    save_config(&cfg);
                                                    open_picker3 = false;
                                                }
                                            });
                                        });
                                }
                                player.show_picker3 = open_picker3;
                            }

                            ui.add_space(10.0);
                            ui.heading("FFT config");
                            ui.separator();

                            if ui
                                .add(
                                    egui::Slider::new(&mut cfg.fft_size, 500..=24576)
                                        .text("fft size"),
                                )
                                .drag_stopped()
                            {
                                save_config(&cfg);
                            }
                            let max_bars = if cfg.old_style { 128 } else { 512 };
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut cfg.spectrum_bars_quantity,
                                        40..=max_bars,
                                    )
                                    .step_by(1.0)
                                    .text("cantidad de barras del visualizador de espectro"),
                                )
                                .drag_stopped()
                            {
                                save_config(&cfg);
                            }
                            if ui.checkbox(&mut cfg.spectrum_smooth, "Suavizado").changed() {
                                save_config(&cfg);
                            }
                            if !cfg.old_style {
                                if ui.checkbox(&mut cfg.line_mode, "Line mode").changed() {
                                    save_config(&cfg);
                                }
                            }
                            if ui.checkbox(&mut cfg.old_style, "Old style").changed() {
                                if cfg.old_style {
                                    cfg.line_mode = false;
                                }
                                save_config(&cfg);
                            }

                            ui.add_space(10.0);
                            ui.heading("Música local");
                            ui.separator();

                            let changed = draw_music_dirs(ui, &mut cfg);

                            if changed {
                                save_config(&cfg);
                                reload_library = true;
                            }
                        }

                        if reload_cover {
                            player.ensure_cover_loaded(&ctx, true);
                        }

                        if reload_library {
                            player.load_library_async();
                        }
                    });
            });

        player.show_settings = show_settings;
    }
}
