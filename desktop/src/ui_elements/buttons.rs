use egui::{Color32, Context, Ui};
use player_core::{PlayerCommand};
use crate::{PlayerApp, utils::marquee_text::show_marquee_text};

pub fn show_buttons_and_title(ui: &mut Ui, ctx: &Context, player_app: &mut PlayerApp, text_color: Color32, accent: Color32) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            let metadata = player_app.player.state.lock().unwrap().metadata.clone();
            if let Some(metadata) = metadata {
                let text = format!("\"{}\" By: {}", metadata.title, metadata.artist);
                show_marquee_text(ui, &text, 40.0, text_color.clone());
            } else {
                show_marquee_text(
                    ui,
                    "\"ReAmped\" — XxAlexplosivoxX",
                    40.0,
                    text_color.clone(),
                );
            }
            ui.horizontal(|ui| {
                let state = player_app.player.state.lock().unwrap();

                let shuffle_on = state.shuffle;
                let repeat_on = state.repeat;
                let repeat_one_on = state.repeat_one;
                let play_on = state.playing;

                drop(state);

                if ui.add(egui::Button::new("⏮")).clicked() {
                    player_app.player.send(PlayerCommand::Prev);
                    player_app.ensure_cover_loaded(ctx, true);
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
                    player_app.player.send(PlayerCommand::Next);
                    player_app.ensure_cover_loaded(ctx, true);
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