use egui::{Color32, Ui};
use player_core::{PlayerCommand, player::Options};
use crate::{PlayerApp};

pub fn show_order_buttons(ui: &mut Ui, player: &mut PlayerApp, _accent: Color32, _text_color: Color32) {
    if ui
        .button("≡ ".to_owned() + player.sort_option.to_string().as_str())
        .clicked()
    {
        let sort_option = player.sort_option.clone();
        match sort_option {
            Options::Normal => {
                player.sort_option = Options::Alphabetical;
            }
            Options::Alphabetical => {
                player.sort_option = Options::Normal;
            }
        }
        player.load_library_async();
    }
    if ui.button("🔀 Shuffle").clicked() {
        player.player.send(PlayerCommand::AleatoryFullRandom);
    }
}
