use egui::Key;
use player_core::{KeyBindings, KeyCode, PlayerCommand};

pub fn egui_key_to_keycode(key: &Key) -> Option<KeyCode> {
    match key {
        Key::Space => Some(KeyCode::Space),
        Key::Enter => Some(KeyCode::Enter),
        Key::ArrowUp => Some(KeyCode::ArrowUp),
        Key::ArrowDown => Some(KeyCode::ArrowDown),
        Key::ArrowLeft => Some(KeyCode::ArrowLeft),
        Key::ArrowRight => Some(KeyCode::ArrowRight),
        _ => None,
    }
}

pub fn handle_keyboard_input(
    ctx: &egui::Context,
    keybindings: &KeyBindings,
    is_playing: bool,
) -> Option<PlayerCommand> {
    if ctx.memory(|mem| mem.focused().is_some()) {
        return None;
    }
    ctx.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    ..
                } => {
                    // Try special keys first
                    if let Some(keycode) = egui_key_to_keycode(key) {
                        if keybindings.is_play_pause_key(&keycode) {
                            return Some(if is_playing {
                                PlayerCommand::Pause
                            } else {
                                PlayerCommand::Play
                            });
                        } else if let Some(cmd) = keybindings.get_command(&keycode) {
                            return Some(cmd);
                        }
                    }
                }
                egui::Event::Text(text) => {
                    // Handle single character keys
                    if text.len() == 1 {
                        let ch = text.chars().next().unwrap();
                        let upper = ch.to_uppercase().to_string();
                        
                        let code = match upper.as_str() {
                            "N" => Some(KeyCode::KeyN),
                            "P" => Some(KeyCode::KeyP),
                            "M" => Some(KeyCode::KeyM),
                            "R" => Some(KeyCode::KeyR),
                            "S" => Some(KeyCode::KeyS),
                            _ => None,
                        };

                        if let Some(keycode) = code {
                            if keybindings.is_play_pause_key(&keycode) {
                                return Some(if is_playing {
                                    PlayerCommand::Pause
                                } else {
                                    PlayerCommand::Play
                                });
                            } else if let Some(cmd) = keybindings.get_command(&keycode) {
                                return Some(cmd);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        None
    })
}
