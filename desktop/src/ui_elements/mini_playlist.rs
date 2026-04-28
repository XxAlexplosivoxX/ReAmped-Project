use player_core::Track;
use egui::{Color32, Ui, scroll_area::ScrollBarVisibility};

use crate::utils::truncate::truncate;

pub fn mini_playlist<F>(
    ui: &mut Ui,
    playlist: &[Track],
    current: Option<Track>,
    playing: bool,
    accent: Color32,
    mut on_select: F,
    pos: f32,
    just_executed: bool,
    search_str: String,
) where
    F: FnMut(usize),
{
    let row_height = 18.0;
    let max_rows = 8;
    let height = row_height * max_rows as f32 + 6.0;

    let search = search_str.to_ascii_lowercase();

    egui::Frame::new()
        .fill(Color32::from_black_alpha(25))
        .corner_radius(2.0)
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .max_width(ui.available_width())
                .max_height(height)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    for (i, track) in playlist.iter().enumerate() {
                        if !search.is_empty() && search.len() >= 3 {
                            if !track.title.to_ascii_lowercase().contains(&search) {
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

                        let resp = ui.add(egui::Button::selectable(is_current, label).fill(accent));

                        if is_current && pos < 0.1 && !just_executed {
                            resp.scroll_to_me(Some(egui::Align::Center));
                        }

                        if resp.clicked() {
                            on_select(i);
                        }
                    }
                });
        });
}
