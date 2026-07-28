use crate::{PlayerApp, ui_elements::mini_playlist::mini_playlist};
use player_core::{Track, PlayerCommand};
use egui::{Color32, RichText, TextEdit, Ui};
use std::{time::Duration, thread::sleep};

pub fn show_search_and_miniplaylist(ui: &mut Ui, player: &mut PlayerApp) {
    if ui
        .horizontal(|ui| {
            let p = &player.palette;
            if !ui
                .add_sized(
                    [ui.available_width() / 3.0, ui.available_height()],
                    TextEdit::singleline(&mut player.search_str)
                        .hint_text(
                            RichText::new("type here to search...")
                                .color(
                                    Color32::from_rgb(p.on_surface[0], p.on_surface[1], p.on_surface[2])
                                    .linear_multiply(0.5),
                                )
                                .italics(),
                        )
                        .background_color(
                            Color32::from_rgba_premultiplied(
                                p.surface_variant[0],
                                p.surface_variant[1],
                                p.surface_variant[2],
                                100,
                            )
                            .linear_multiply(0.5),
                        ),
                )
                .contains_pointer()
            {
                // self.search_str = String::from("")
            }
            let playlist = player.player.playlist();
            mini_playlist(
                ui,
                &playlist,
                player.current_track(),
                player.player.is_playing(),
                &player.palette,
                |track: &Track| player.player.send(PlayerCommand::JumpToPath(track.path.clone())),
                player.position,
                player.just_executed,
                player.search_str.clone(),
                &mut player.scroll_current_track,
            );
        })
        .response
        .changed()
    {
        sleep(Duration::from_secs(2));
        player.search_str = String::from("");
    };
}
