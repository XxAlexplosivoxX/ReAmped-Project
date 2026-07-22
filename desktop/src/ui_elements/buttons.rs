use egui::{Color32, Context, Ui};
use player_core::{PlayerCommand};
use crate::{PlayerApp, utils::marquee_text::show_marquee_text_cached};

pub fn show_buttons_and_title(ui: &mut Ui, ctx: &Context, player_app: &mut PlayerApp, text_color: Color32, accent: Color32) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            let metadata = player_app.player.metadata();
            let text = if let Some(ref metadata) = metadata {
                format!("\"{}\" By: {}", metadata.title, metadata.artist)
            } else {
                String::from("\"ReAmped\" — XxAlexplosivoxX")
            };
            show_marquee_text_cached(
                ui,
                &text,
                &mut player_app.marquee_cache_text,
                &mut player_app.marquee_cache_galley,
                &mut player_app.marquee_cache_width,
                40.0,
                text_color,
            );
            ui.horizontal(|ui| {
                let shuffle_on = player_app.player.shuffle();
                let repeat_on = player_app.player.repeat();
                let repeat_one_on = player_app.player.repeat_one();
                let play_on = player_app.player.is_playing();

                if ui.add(egui::Button::new("⏮")).clicked() {
                    let old_idx = player_app.player.playlist_idx();
                    player_app.player.send(PlayerCommand::Prev);
                    // Wait for state to actually change
                    for _ in 0..50 {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        if player_app.player.playlist_idx() != old_idx {
                            break;
                        }
                    }
                    player_app.ensure_cover_loaded(ctx, true);
                    ctx.request_repaint();
                }

                if ui.add(egui::Button::new("⏹")).clicked() {
                    player_app.player.send(PlayerCommand::Stop);
                }

                if ui
                    .add(egui::Button::selectable(
                        play_on,
                        egui::RichText::new("▶").color(if play_on {
                            accent.linear_multiply(2.4)
                        } else {
                            ui.visuals().text_color()
                        }),
                    ))
                    .clicked()
                {
                    player_app.player.send(PlayerCommand::Play);
                }

                if ui
                    .add(egui::Button::selectable(
                        !play_on,
                        egui::RichText::new("⏸").color(if !play_on {
                            accent.linear_multiply(2.4)
                        } else {
                            ui.visuals().text_color()
                        }),
                    ))
                    .clicked()
                {
                    player_app.player.send(PlayerCommand::Pause);
                }

                if ui.add(egui::Button::new("⏭")).clicked() {
                    let old_idx = player_app.player.playlist_idx();
                    player_app.player.send(PlayerCommand::Next);
                    // Wait for state to actually change
                    for _ in 0..50 {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        if player_app.player.playlist_idx() != old_idx {
                            break;
                        }
                    }
                    player_app.ensure_cover_loaded(ctx, true);
                    ctx.request_repaint();
                }
                ui.style_mut().visuals.widgets.noninteractive.bg_stroke =
                    egui::Stroke::new(1.0, text_color);
                ui.separator();

                if ui
                    .add(egui::Button::selectable(
                        shuffle_on,
                        egui::RichText::new("🔀").color(if shuffle_on {
                            accent.linear_multiply(2.4)
                        } else {
                            ui.visuals().text_color()
                        }),
                    ))
                    .clicked()
                {
                    player_app.player.send(PlayerCommand::ToggleShuffle);
                }

                if ui
                    .add(egui::Button::selectable(
                        repeat_on,
                        egui::RichText::new("🔁").color(if repeat_on {
                            accent.linear_multiply(2.4)
                        } else {
                            ui.visuals().text_color()
                        }),
                    ))
                    .clicked()
                {
                    player_app.player.send(PlayerCommand::ToggleRepeat);
                }

                if ui
                    .add(egui::Button::selectable(
                        repeat_one_on,
                        egui::RichText::new("🔂").color(if repeat_one_on {
                            accent.linear_multiply(2.4)
                        } else {
                            ui.visuals().text_color()
                        }),
                    ))
                    .clicked()
                {
                    player_app.player.send(PlayerCommand::ToggleRepeatOne);
                }
            });
        });
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.button("🔄 rescan").clicked() {
                    player_app.load_library_async();
                }
                if ui.button(if player_app.fullscreen { "🗖" } else { "🗗" }).clicked() {
                    player_app.fullscreen = !player_app.fullscreen;

                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(player_app.fullscreen));
                }
                if ui.button("⚙").clicked() {
                    player_app.show_settings = true;
                }
            });
        });
    });
}