use egui::Color32;
use player_core::{PlayerCommand, Options};
use crate::PlayerApp;

pub fn show_order_buttons(ui: &mut egui::Ui, player: &mut PlayerApp) {
    if ui
        .add(egui::Button::new("≡ ".to_owned() + player.sort_option.to_string().as_str()).fill(Color32::TRANSPARENT))
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
    if ui.add(egui::Button::new("🔀 Shuffle").fill(Color32::TRANSPARENT)).clicked() {
        player.player.send(PlayerCommand::AleatoryFullRandom);
    }
}
