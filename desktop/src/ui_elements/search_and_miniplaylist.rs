use crate::{
    PlayerApp,
    ui_elements::mini_playlist::{MiniPlaylistPlayback, mini_playlist},
};
use egui::{Color32, RichText, TextEdit, Ui};
use player_core::{PlayerCommand, Track};
use std::{thread::sleep, time::Duration};

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
                                    Color32::from_rgb(
                                        p.on_surface[0],
                                        p.on_surface[1],
                                        p.on_surface[2],
                                    )
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
                MiniPlaylistPlayback {
                    current: player.current_track(),
                    playing: player.player.is_playing(),
                    pos: player.position,
                    just_executed: player.just_executed,
                },
                &player.palette,
                |track: &Track| {
                    player
                        .player
                        .send(PlayerCommand::JumpToPath(track.path.clone()))
                },
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
