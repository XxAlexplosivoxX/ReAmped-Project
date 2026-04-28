use crate::PlayerApp;
use egui::{Label, Slider, Ui, style::HandleShape};
use player_core::{PlayerCommand, config::save_config};

pub fn show_volume_bar(ui: &mut Ui, player: &mut PlayerApp) {
    ui.add_sized(
        [39.8, 20.5],
        Label::new("🔊 ".to_owned() + format!("{:.0}%", player.volume * 100.0).as_str()),
    );
    let resp = ui.add(
        Slider::new(&mut player.volume, 0.0..=1.0)
            .show_value(false)
            .step_by(0.01)
            .handle_shape(HandleShape::Rect {
                aspect_ratio: (1.0),
            })
            .trailing_fill(true),
    );
    {
        let mut cfg = player.config.lock().unwrap();
        if resp.changed() {
            player.player.send(PlayerCommand::SetVolume(player.volume));
            cfg.volume = player.volume;
        } else if resp.drag_stopped() {
            save_config(&cfg);
        }
    }
}
