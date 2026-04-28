use player_core::config::{AppConfig, save_config};

pub fn draw_music_dirs(ui: &mut egui::Ui, config: &mut AppConfig) -> bool {
    let mut changed = false;
    let mut to_remove = None;

    for (i, dir) in config.music_dirs.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(dir.display().to_string());

            if ui.small_button("❌").clicked() {
                to_remove = Some(i);
            }
        });
    }

    if let Some(i) = to_remove {
        config.music_dirs.remove(i);
        save_config(config);
        changed = true;
    }

    if ui.button("➕ Add folder").clicked() {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            config.music_dirs.push(path);
            save_config(config);
            changed = true;
        }
    }

    changed
}