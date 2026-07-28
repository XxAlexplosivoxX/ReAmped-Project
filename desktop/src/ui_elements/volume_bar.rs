use crate::PlayerApp;
use egui::{Color32, Label, Slider, Ui, style::HandleShape};
use player_core::{PlayerCommand, config::save_config};

pub fn show_volume_bar(ui: &mut Ui, player: &mut PlayerApp) {
    ui.add_sized(
        [39.8, 20.5],
        Label::new("🔊 ".to_owned() + format!("{:.0}%", player.volume * 100.0).as_str()),
    );
    let on_primary_col = Color32::from_rgb(
        player.palette.on_primary[0],
        player.palette.on_primary[1],
        player.palette.on_primary[2],
    );
    let prev_inactive = ui.style().visuals.widgets.inactive.bg_fill;
    ui.style_mut().visuals.widgets.inactive.bg_fill = on_primary_col;
    let resp = ui.add(
        Slider::new(&mut player.volume, 0.0..=1.0)
            .show_value(false)
            .step_by(0.01)
            .handle_shape(HandleShape::Rect {
                aspect_ratio: (1.0),
            })
            .trailing_fill(true),
    );
    ui.style_mut().visuals.widgets.inactive.bg_fill = prev_inactive;
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
