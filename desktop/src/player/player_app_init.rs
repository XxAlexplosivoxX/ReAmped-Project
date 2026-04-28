use std::sync::{Arc, Mutex};
use egui::Color32;
use player_core::{
    Player, PlayerCommand, Track,
    player::Options,
};
use crate::{utils::{load_cover::load_cover_texture, misc::extract_palette, luminance::luminance, scan_music_dirs::scan_music_dirs, visualizer::SpectrumVisualizer}};
use player_core::config::{AppConfig, load_config};

#[derive(Clone)]
pub struct PlayerApp {
    pub player: Player,
    pub volume: f32,
    pub visualizer: SpectrumVisualizer,
    pub cover_texture: Option<egui::TextureHandle>,
    pub current_track: Option<Track>,
    pub position: f32,
    pub palette: Vec<[u8; 3]>,
    pub palette_sorted: Vec<[u8; 3]>,
    pub state: String,
    pub text_color: Color32,
    pub fullscreen: bool,
    pub show_settings: bool,
    pub just_executed: bool,
    pub config: Arc<Mutex<AppConfig>>,
    pub rgb1: [f32; 3],
    pub rgb2: [f32; 3],
    pub rgb3: [f32; 3],
    pub show_picker1: bool,
    pub show_picker2: bool,
    pub show_picker3: bool,
    pub search_str: String,
    pub sort_option: Options,
    pub bass_val: f32,
    pub mid_val: f32,
    pub high_val: f32,
    pub width_val: f32
}

impl Default for PlayerApp {
    fn default() -> Self {
        let config = Arc::new(Mutex::new(load_config()));
        Self {
            player: Player::new(load_config().volume),
            volume: load_config().volume,
            visualizer: SpectrumVisualizer::new(config.clone()),
            cover_texture: None,
            current_track: None,
            position: 0.0,
            palette: vec![[0, 0, 0], [0, 0, 0], [0, 0, 0]],
            palette_sorted: vec![[0, 0, 0], [0, 0, 0], [0, 0, 0]],
            state: String::from("status: Welcome"),
            text_color: Color32::WHITE,
            fullscreen: false,
            show_settings: false,
            just_executed: true,
            config: config,
            rgb1: [
                load_config().theme.pallete_custom[0][0] as f32 / 255.0,
                load_config().theme.pallete_custom[0][1] as f32 / 255.0,
                load_config().theme.pallete_custom[0][2] as f32 / 255.0,
            ],
            rgb2: [
                load_config().theme.pallete_custom[1][0] as f32 / 255.0,
                load_config().theme.pallete_custom[1][1] as f32 / 255.0,
                load_config().theme.pallete_custom[1][2] as f32 / 255.0,
            ],
            rgb3: [
                load_config().theme.pallete_custom[2][0] as f32 / 255.0,
                load_config().theme.pallete_custom[2][1] as f32 / 255.0,
                load_config().theme.pallete_custom[2][2] as f32 / 255.0,
            ],
            show_picker1: false,
            show_picker2: false,
            show_picker3: false,
            search_str: String::from(""),
            sort_option: Options::Normal,
            bass_val: 1.0, 
            mid_val: 1.0, 
            high_val: 1.0, 
            width_val: 1.0
        }
    }
}

impl PlayerApp {
    pub fn ensure_cover_loaded(&mut self, ctx: &egui::Context, ovride: bool) {
        let cfg = self.config.lock().unwrap();
        let current_track = {
            let state = self.player.state.lock().unwrap();
            let playing = state.playing;
            drop(state);

            let pl = self.player.playlist();
            let idx = self.player.playlist_idx();

            if playing && !pl.is_empty() {
                Some(pl[idx].clone())
            } else {
                None
            }
        };
        let should_reload = ovride
            || self.current_track.as_ref().map(|t| &t.path)
                != current_track.as_ref().map(|t| &t.path)
            || self.current_track.is_none();

        if should_reload {
            let cover = self.player.cover();
            self.cover_texture = Some(load_cover_texture(ctx, &cover).unwrap());
            self.current_track = current_track;
            if cfg.theme.follow_cover {
                self.palette = extract_palette(cover);
                self.palette_sorted = self.palette.clone();
                self.palette_sorted
                    .sort_by(|a, b| luminance(*a).partial_cmp(&luminance(*b)).unwrap());
            } else {
                self.palette = cfg.theme.pallete_custom.clone();
                self.palette_sorted = cfg.theme.pallete_custom.clone();
                self.palette_sorted
                    .sort_by(|a, b| luminance(*a).partial_cmp(&luminance(*b)).unwrap());
            }
            let palette = self.palette_sorted.clone();
            let panel = Color32::from_rgba_unmultiplied_const(
                palette[2][0],
                palette[2][1],
                palette[2][2],
                100,
            );
            let accent = Color32::from_rgba_unmultiplied_const(
                palette[1][0],
                palette[1][1],
                palette[1][2],
                100,
            );
            let text = Color32::from_rgb(palette[0][0], palette[0][1], palette[0][2]);
            self.text_color = text;

            let mut visuals = egui::Visuals::dark();

            visuals.window_fill = Color32::TRANSPARENT;
            visuals.panel_fill = Color32::TRANSPARENT;
            visuals.extreme_bg_color = Color32::TRANSPARENT;

            visuals.button_frame = true;

            visuals.widgets.noninteractive.bg_fill = panel.linear_multiply(1.05);
            visuals.widgets.noninteractive.fg_stroke.color = text;
            visuals.widgets.noninteractive.weak_bg_fill = panel.linear_multiply(1.05);

            visuals.widgets.inactive.bg_fill = panel.linear_multiply(1.05);
            visuals.widgets.inactive.fg_stroke.color = text;
            visuals.widgets.inactive.weak_bg_fill = panel.linear_multiply(1.05);

            visuals.widgets.hovered.bg_fill = accent.linear_multiply(0.65);
            visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
            visuals.widgets.hovered.weak_bg_fill = accent.linear_multiply(0.65);

            visuals.widgets.active.bg_fill = accent;
            visuals.override_text_color = Some(text.linear_multiply(1.2));

            visuals.widgets.inactive.bg_stroke.color = accent.linear_multiply(0.8);
            visuals.widgets.active.fg_stroke.color = accent;
            visuals.widgets.hovered.fg_stroke.color = accent;

            visuals.widgets.active.weak_bg_fill = accent;
            visuals.widgets.hovered.weak_bg_fill = accent;
            visuals.selection.bg_fill = text.gamma_multiply(0.9);

            ctx.set_visuals(visuals);
        }
    }
    pub fn load_library_async(&self) {
        let cfg = self.config.lock().unwrap();
        let sender = self.player.clone();
        let dirs = cfg.music_dirs.clone();
        let sort_option = self.sort_option.clone();

        std::thread::spawn(move || {
            let tracks = scan_music_dirs(&dirs);

            if !tracks.is_empty() {
                sender.send(PlayerCommand::SetPlaylist(tracks));
                sender.send(PlayerCommand::SortBy(sort_option));
            }
        });
    }
}