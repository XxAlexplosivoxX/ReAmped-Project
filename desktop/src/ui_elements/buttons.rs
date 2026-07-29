use egui::{Color32, Context, Ui};
use player_core::PlayerCommand;
use crate::PlayerApp;
use crate::utils::marquee_text::show_marquee_text_cached;

fn sel_button(ui: &mut Ui, selected: bool, label: &str, on_color: Color32, off_color: Color32) -> egui::Response {
    if selected {
        ui.add(
            egui::Button::selectable(true, egui::RichText::new(label).color(off_color))
                .fill(on_color),
        )
    } else {
        ui.add(
            egui::Button::selectable(false, egui::RichText::new(label).color(off_color))
                .fill(Color32::TRANSPARENT),
        )
    }
}

pub fn show_buttons_and_title(ui: &mut Ui, ctx: &Context, player_app: &mut PlayerApp, text_color: Color32) {
    let p = &player_app.palette;
    let on_bg = Color32::from_rgb(p.primary[0], p.primary[1], p.primary[2]);
    let off_fg = Color32::from_rgb(p.on_surface[0], p.on_surface[1], p.on_surface[2]);

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
                Color32::from_rgb(off_fg[0], off_fg[1], off_fg[2]),
            );
            ui.horizontal(|ui| {
                let shuffle_on = player_app.player.shuffle();
                let repeat_on = player_app.player.repeat();
                let repeat_one_on = player_app.player.repeat_one();
                let play_on = player_app.player.is_playing();

                if ui.add(egui::Button::new("⏮").fill(Color32::TRANSPARENT)).clicked() {
                    player_app.player.send(PlayerCommand::Prev);
                    player_app.ensure_cover_loaded(ctx, true);
                    ctx.request_repaint();
                }

                if ui.add(egui::Button::new("⏹").fill(Color32::TRANSPARENT)).clicked() {
                    player_app.player.send(PlayerCommand::Stop);
                }

                if sel_button(ui, play_on, "▶", on_bg, off_fg).clicked() {
                    player_app.player.send(PlayerCommand::Play);
                }

                if sel_button(ui, !play_on, "⏸", on_bg, off_fg).clicked() {
                    player_app.player.send(PlayerCommand::Pause);
                }

                if ui.add(egui::Button::new("⏭").fill(Color32::TRANSPARENT)).clicked() {
                    player_app.player.send(PlayerCommand::Next);
                    player_app.ensure_cover_loaded(ctx, true);
                    ctx.request_repaint();
                }
                ui.style_mut().visuals.widgets.noninteractive.bg_stroke =
                    egui::Stroke::new(1.0, text_color);
                ui.separator();

                if sel_button(ui, shuffle_on, "🔀", on_bg, off_fg).clicked() {
                    player_app.player.send(PlayerCommand::ToggleShuffle);
                }

                if sel_button(ui, repeat_on, "🔁", on_bg, off_fg).clicked() {
                    player_app.player.send(PlayerCommand::ToggleRepeat);
                }

                if sel_button(ui, repeat_one_on, "🔂", on_bg, off_fg).clicked() {
                    player_app.player.send(PlayerCommand::ToggleRepeatOne);
                }
            });
        });
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("🔄 rescan").fill(Color32::TRANSPARENT)).clicked() {
                    player_app.load_library_async();
                }
                if ui.add(egui::Button::new(if player_app.fullscreen { "🗖" } else { "🗗" }).fill(Color32::TRANSPARENT)).clicked() {
                    player_app.fullscreen = !player_app.fullscreen;

                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(player_app.fullscreen));
                }
                if ui.add(egui::Button::new("⚙").fill(Color32::TRANSPARENT)).clicked() {
                    player_app.show_settings = true;
                }
            });
        });
    });
}