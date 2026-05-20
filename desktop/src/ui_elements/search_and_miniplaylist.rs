use crate::{PlayerApp, ui_elements::mini_playlist::mini_playlist};
use player_core::{Track, PlayerCommand};
use egui::{Color32, RichText, TextEdit, Ui};
use std::{time::Duration, thread::sleep};

pub fn show_search_and_miniplaylist(ui: &mut Ui, player: &mut PlayerApp, accent: Color32) {
    if ui
        .horizontal(|ui| {
            if !ui
                .add_sized(
                    [ui.available_width() / 3.0, ui.available_height()],
                    TextEdit::singleline(&mut player.search_str)
                        .hint_text(
                            RichText::new("type here to search...")
                                .color(
                                    Color32::from_rgb(
                                        player.palette_sorted[0][0],
                                        player.palette_sorted[0][1],
                                        player.palette_sorted[0][2],
                                    )
                                    .linear_multiply(0.5),
                                )
                                .italics(),
                        )
                        .background_color(
                            Color32::from_rgba_premultiplied(
                                player.palette_sorted[1][0],
                                player.palette_sorted[1][1],
                                player.palette_sorted[1][2],
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
                player.current_track.clone(),
                player.player.is_playing(),
                accent,
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
