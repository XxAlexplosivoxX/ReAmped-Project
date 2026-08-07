use egui::{Color32, Context};
use player_core::config::{save_config, M3Palette, ThemeSource};

use crate::{ui_elements::music_dirs::draw_music_dirs, player::player_app_init::PlayerApp};

macro_rules! m3_role {
    ($name:expr, $get:ident, $get_mut:ident) => {
        ($name, |p: &M3Palette| &p.$get, |p: &mut M3Palette| &mut p.$get_mut)
    };
}

/// All M3 colour roles in display order.
const M3_ROLES: &[(&str, fn(&M3Palette) -> &[u8; 3], fn(&mut M3Palette) -> &mut [u8; 3])] = &[
    m3_role!("Primary", primary, primary),
    m3_role!("On-Primary", on_primary, on_primary),
    m3_role!("Primary Container", primary_container, primary_container),
    m3_role!("On-Primary Container", on_primary_container, on_primary_container),
    m3_role!("Secondary", secondary, secondary),
    m3_role!("On-Secondary", on_secondary, on_secondary),
    m3_role!("Secondary Container", secondary_container, secondary_container),
    m3_role!("On-Secondary Container", on_secondary_container, on_secondary_container),
    m3_role!("Tertiary", tertiary, tertiary),
    m3_role!("On-Tertiary", on_tertiary, on_tertiary),
    m3_role!("Tertiary Container", tertiary_container, tertiary_container),
    m3_role!("On-Tertiary Container", on_tertiary_container, on_tertiary_container),
    m3_role!("Error", error, error),
    m3_role!("On-Error", on_error, on_error),
    m3_role!("Error Container", error_container, error_container),
    m3_role!("On-Error Container", on_error_container, on_error_container),
    m3_role!("Surface", surface, surface),
    m3_role!("On-Surface", on_surface, on_surface),
    m3_role!("Surface Variant", surface_variant, surface_variant),
    m3_role!("On-Surface Variant", on_surface_variant, on_surface_variant),
    m3_role!("Outline", outline, outline),
    m3_role!("Outline Variant", outline_variant, outline_variant),
    m3_role!("Background", background, background),
    m3_role!("On-Background", on_background, on_background),
];

fn draw_role_editor(
    ui: &mut egui::Ui,
    _name: &str,
    color: &mut [u8; 3],
    label: &str,
    expanded: &mut bool,
) {
    ui.horizontal(|ui| {
        let swatch = Color32::from_rgb(color[0], color[1], color[2]);
        let resp = ui.colored_label(swatch, "████");
        if resp.clicked() {
            *expanded = !*expanded;
        }
        ui.label(label);
    });

    if *expanded {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.add(
                    egui::Slider::new(&mut color[0], 0..=255)
                        .text("R")
                        .fixed_decimals(0),
                );
                ui.add(
                    egui::Slider::new(&mut color[1], 0..=255)
                        .text("G")
                        .fixed_decimals(0),
                );
                ui.add(
                    egui::Slider::new(&mut color[2], 0..=255)
                        .text("B")
                        .fixed_decimals(0),
                );
            });
        });
    }
}

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

                            // Theme source selector
                            let prev_source = cfg.theme.source.clone();
                            ui.label("Fuente de la paleta de colores:");
                            ui.radio_value(&mut cfg.theme.source, ThemeSource::AlbumCover, "Portada del tema (AlbumCover)");
                            ui.radio_value(&mut cfg.theme.source, ThemeSource::SystemWallpaper, "Fondo de pantalla (SystemWallpaper)");
                            ui.radio_value(&mut cfg.theme.source, ThemeSource::Manual, "Ajuste manual (Manual)");

                            if cfg.theme.source != prev_source {
                                save_config(&cfg);
                                reload_cover = true;
                            }

                            ui.add_space(6.0);

                            // Manual colour-role editors
                            if cfg.theme.source == ThemeSource::Manual {
                                ui.label("Roles de color M3 (haz clic en ████ para expandir):");
                                for (label, _, get_mut) in M3_ROLES {
                                    let key = format!("theme_{}", label);
                                    let mut expanded = player.expanded_roles.contains(&key);
                                    let color: &mut [u8; 3] = get_mut(&mut cfg.theme.palette);
                                    draw_role_editor(ui, label, color, label, &mut expanded);
                                    if expanded {
                                        player.expanded_roles.insert(key.clone());
                                    } else {
                                        player.expanded_roles.remove(&key);
                                    }
                                }
                                ui.add_space(4.0);
                                if ui.button("Guardar paleta manual").clicked() {
                                    cfg.theme.source = ThemeSource::Manual;
                                    // Copy the current palette from config to ensure it's saved
                                    save_config(&cfg);
                                    reload_cover = true;
                                }
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

                            if ui
                                .add(
                                    egui::Slider::new(&mut cfg.target_fps, 15..=240)
                                        .text("FPS objetivo"),
                                )
                                .drag_stopped()
                            {
                                save_config(&cfg);
                            }
                            if ui.checkbox(&mut cfg.vsync, "Vsync").changed() {
                                save_config(&cfg);
                            }

                            ui.add_space(10.0);
                            ui.heading("Reproducción");
                            ui.separator();

                            if ui
                                .checkbox(&mut cfg.crossfade_enabled, "Transición suave (crossfade)")
                                .changed()
                            {
                                save_config(&cfg);
                            }

                            if cfg.crossfade_enabled {
                                if ui
                                    .add(
                                        egui::Slider::new(&mut cfg.crossfade_seconds, 1.0..=30.0)
                                            .step_by(0.5)
                                            .text("Duración (segundos)"),
                                    )
                                    .drag_stopped()
                                {
                                    save_config(&cfg);
                                }
                            }

                            if ui
                                .checkbox(&mut cfg.silence_trim_enabled, "Recortar silencio inicial/final")
                                .changed()
                            {
                                save_config(&cfg);
                            }

                            #[cfg(all(target_os = "linux", feature = "bit-perfect-backend"))]
                            {
                                use player_core::PlayerCommand;

                                ui.add_space(6.0);
                                let mut apply_backend = false;
                                if ui
                                    .checkbox(
                                        &mut cfg.bit_perfect_enabled,
                                        "Salida bit-perfect (ALSA directo, sin resampleo)",
                                    )
                                    .changed()
                                {
                                    apply_backend = true;
                                }
                                if cfg.bit_perfect_enabled {
                                    let devices =
                                        player_core::audio::alsa_backend::AlsaBackend::list_devices();
                                    let mut selected = if cfg.bit_perfect_device.trim().is_empty() {
                                        "auto".to_string()
                                    } else {
                                        cfg.bit_perfect_device.clone()
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label("Dispositivo:");
                                        let mut changed = false;
                                        egui::ComboBox::from_id_salt("bit_perfect_device")
                                            .selected_text(if selected == "auto" {
                                                "Auto (detectar)".to_string()
                                            } else {
                                                selected.clone()
                                            })
                                            .show_ui(ui, |ui| {
                                                if ui
                                                    .selectable_label(selected == "auto", "Auto (detectar)")
                                                    .clicked()
                                                {
                                                    selected = "auto".to_string();
                                                    changed = true;
                                                }
                                                for (display, value) in &devices {
                                                    if ui
                                                        .selectable_label(
                                                            *value == selected,
                                                            display,
                                                        )
                                                        .clicked()
                                                    {
                                                        selected = value.clone();
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        if changed {
                                            cfg.bit_perfect_device = if selected == "auto" {
                                                String::new()
                                            } else {
                                                selected.clone()
                                            };
                                            apply_backend = true;
                                        }
                                    });
                                    if cfg.bit_perfect_device.trim().is_empty() && !devices.is_empty() {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Se usará: {}",
                                                devices[0].0
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                    }
                                    ui.label(
                                        egui::RichText::new(
                                            "Requiere una tarjeta que acepte la frecuencia y formato nativos \
                                             del archivo; si no, se usa CPAL como respaldo.",
                                        )
                                        .small()
                                        .weak(),
                                    );
                                }
                                if apply_backend {
                                    save_config(&cfg);
                                    player.player.send(PlayerCommand::SetBitPerfectBackend {
                                        enabled: cfg.bit_perfect_enabled,
                                        device: cfg.bit_perfect_device.clone(),
                                    });
                                }
                            }

                            ui.add_space(10.0);
                            ui.heading("Atajos de teclado");
                            ui.separator();

                            let all_keys = [
                                ("Space", "Espacio"),
                                ("Enter", "Enter"),
                                ("ArrowUp", "↑"),
                                ("ArrowDown", "↓"),
                                ("ArrowLeft", "←"),
                                ("ArrowRight", "→"),
                                ("N", "N"),
                                ("P", "P"),
                                ("M", "M"),
                                ("R", "R"),
                                ("S", "S"),
                            ];

                            let all_commands: &[(&str, &str)] = &[
                                ("PlayPause", "Play/Pause"),
                                ("Play", "Reproducir"),
                                ("Pause", "Pausa"),
                                ("Next", "Siguiente"),
                                ("Previous", "Anterior"),
                                ("Stop", "Detener"),
                                ("Shuffle", "Aleatorio"),
                                ("Repeat", "Repetir"),
                                ("RepeatOne", "Repetir una"),
                            ];

                            let mut kb_changed = false;

                            for (key, key_label) in &all_keys {
                                let current_key = key.to_string();
                                let current_cmd = cfg.keybindings.bindings.get(&current_key).cloned();
                                let current_text = current_cmd.as_deref().map_or("—".to_string(), |c| {
                                    all_commands
                                        .iter()
                                        .find(|(k, _)| *k == c)
                                        .map_or(c.to_string(), |(_, l)| l.to_string())
                                });

                                ui.horizontal(|ui| {
                                    ui.label(format!("{:<12}", *key_label));
                                    egui::ComboBox::from_id_salt(format!("kb_{}", current_key))
                                        .selected_text(&current_text)
                                        .width(140.0)
                                        .show_ui(ui, |ui| {
                                            let is_none = current_cmd.is_none();
                                            if ui
                                                .selectable_label(is_none, "— Sin asignar —")
                                                .clicked() && !is_none
                                            {
                                                cfg.keybindings.bindings.remove(&current_key);
                                                kb_changed = true;
                                            }
                                            for (cmd, cmd_label) in all_commands {
                                                let is_selected = current_cmd.as_deref() == Some(cmd);
                                                if ui
                                                    .selectable_label(is_selected, *cmd_label)
                                                    .clicked() && !is_selected
                                                {
                                                    cfg.keybindings
                                                        .bindings
                                                        .insert(current_key.clone(), cmd.to_string());
                                                    kb_changed = true;
                                                }
                                            }
                                        });
                                });
                            }

                            if kb_changed {
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
