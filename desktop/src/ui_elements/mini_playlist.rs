use egui::{Color32, scroll_area::ScrollBarVisibility};

use player_core::config::M3Palette;
use player_core::Track;

use crate::utils::truncate::truncate;

pub fn mini_playlist<F>(
    ui: &mut egui::Ui,
    playlist: &[Track],
    current: Option<Track>,
    playing: bool,
    palette: &M3Palette,
    mut on_select: F,
    pos: f32,
    just_executed: bool,
    search_str: String,
    scroll_current_track: &mut bool,
) where
    F: FnMut(&Track),
{
    let row_height = 18.0;
    let max_rows = 8;
    let height = row_height * max_rows as f32 + 6.0;

    let search = search_str.to_ascii_lowercase();

    let bg = Color32::from_rgb(palette.surface_variant[0], palette.surface_variant[1], palette.surface_variant[2]);
    let fg = Color32::from_rgb(palette.on_surface[0], palette.on_surface[1], palette.on_surface[2]);
    let sel_bg = Color32::from_rgb(palette.primary[0], palette.primary[1], palette.primary[2]);
    let sel_fg = Color32::from_rgb(palette.on_primary[0], palette.on_primary[1], palette.on_primary[2]);

    egui::Frame::new()
        .fill(Color32::from_black_alpha(25))
        .corner_radius(2.0)
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .max_width(ui.available_width())
                .max_height(height)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    for (_i, track) in playlist.iter().enumerate() {
                        if !search.is_empty() && search.len() >= 3 {
                            if !track.title.to_ascii_lowercase().contains(&search)
                                && !track.artist.to_ascii_lowercase().contains(&search)
                            {
                                continue;
                            }
                        }

                        let is_current = current.as_ref().map_or(false, |c| c.path == track.path);

                        let icon = if is_current {
                            if playing { "▶" } else { "⏸" }
                        } else {
                            ""
                        };

                        let label = format!("{} {}", icon, truncate(&track.title, 20));

                        let (fill, text_col) = if is_current { (sel_bg, sel_fg) } else { (bg, fg) };

                        let resp = ui.add(
                            egui::Button::selectable(is_current, egui::RichText::new(label).color(text_col))
                                .fill(fill),
                        );

                        if is_current && ((pos < 0.1 && !just_executed) || *scroll_current_track) {
                            resp.scroll_to_me(Some(egui::Align::Center));
                            *scroll_current_track = false;
                        }

                        if resp.clicked() {
                            on_select(track);
                        }
                    }
                });
        });
}
