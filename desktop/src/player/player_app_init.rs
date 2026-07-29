use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use std::time::Duration;
use std::thread;
use egui::{Color32, ColorImage};
use player_core::{
    Player, PlayerBuilder, PlayerCommand, Track, Options, metadata::CoverArt,
};
use player_core::config::{AppConfig, load_config, M3Palette, ThemeSource};
use player_core::metadata::is_default_cover;
use crate::utils::{
    media_controls::{MediaControls, MediaSnapshot},
    misc::{extract_palette, extract_palette_from_bytes, find_folder_cover, get_system_wallpaper_buffer},
    visualizer::SpectrumVisualizer,
    scan_music_dirs::scan_music_dirs,
};

pub struct CoverWorkResult {
    pub cover_data: Vec<u8>,
    pub color_image: Option<ColorImage>,
    pub palette: M3Palette,
}

pub type SharedCoverResult = Arc<Mutex<Option<CoverWorkResult>>>;

#[derive(Clone)]
pub struct PlayerApp {
    pub player: Player,
    pub volume: f32,
    pub visualizer: SpectrumVisualizer,
    pub media_controls: MediaControls,
    pub cover_texture: Option<egui::TextureHandle>,
    pub previous_cover_data: Vec<u8>,
    pub position: f32,
    /// Current animated M3 palette (lerps toward target_palette each frame).
    pub palette: M3Palette,
    /// Target M3 palette that `palette` animates toward.
    pub target_palette: M3Palette,
    pub state: &'static str,
    pub text_color: Color32,
    pub fullscreen: bool,
    pub show_settings: bool,
    pub just_executed: bool,
    pub config: Arc<Mutex<AppConfig>>,
    pub search_str: String,
    pub sort_option: Options,
    pub bass_val: f32,
    pub mid_val: f32,
    pub high_val: f32,
    pub width_val: f32,
    pub startup_tracks: Vec<Track>,
    pub library_loading: bool,
    pub scroll_current_track: bool,
    pub bg_anim_t: f32,
    pub marquee_cache_text: String,
    pub marquee_cache_galley: Option<std::sync::Arc<egui::Galley>>,
    pub marquee_cache_width: f32,
    /// Which M3 colour roles have their inline picker expanded (for manual editing).
    pub expanded_roles: HashSet<String>,
    pub show_palette_debug: bool,
    pub last_scrolled_track: Option<std::path::PathBuf>,
    pub pending_cover_result: SharedCoverResult,
}

impl Default for PlayerApp {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl PlayerApp {
    pub fn new(startup_tracks: Vec<Track>) -> Self {
        let config_values = load_config();
        let config = Arc::new(Mutex::new(config_values.clone()));
        let visualizer = SpectrumVisualizer::new(config.clone());
        let player = PlayerBuilder::new().with_volume(config_values.volume).build();
        let media_controls = MediaControls::start(player.clone());
        let default_palette = M3Palette::default();

        let app = Self {
            player,
            volume: config_values.volume,
            visualizer,
            media_controls,
            cover_texture: None,
            previous_cover_data: Vec::new(),
            position: 0.0,
            palette: default_palette.clone(),
            target_palette: default_palette,
            state: "status: Welcome",
            text_color: Color32::WHITE,
            fullscreen: config_values.fullscreen,
            show_settings: false,
            just_executed: true,
            config,
            search_str: String::from(""),
            sort_option: Options::Normal,
            bass_val: config_values.bass_val,
            mid_val: config_values.mid_val,
            high_val: config_values.high_val,
            width_val: config_values.width_val,
            startup_tracks,
            library_loading: false,
            scroll_current_track: false,
            bg_anim_t: 0.0,
            marquee_cache_text: String::new(),
            marquee_cache_galley: None,
            marquee_cache_width: 0.0,
            expanded_roles: HashSet::new(),
            show_palette_debug: false,
            last_scrolled_track: None,
            pending_cover_result: Arc::new(Mutex::new(None)),
        };

        app.player.send(PlayerCommand::SetGainBass(app.bass_val));
        app.player.send(PlayerCommand::SetGainMid(app.mid_val));
        app.player.send(PlayerCommand::SetGainHigh(app.high_val));
        app.player.send(PlayerCommand::SetExpanderWidth(app.width_val));

        app.spawn_media_sync_thread();
        app
    }

    fn spawn_media_sync_thread(&self) {
        let player = self.player.clone();
        let media_controls = self.media_controls.clone();

        thread::spawn(move || {
            loop {
                let playlist = player.playlist();
                let playlist_idx = player.playlist_idx();
                let current_track = playlist.get(playlist_idx).cloned();

                media_controls.sync_from_snapshot(MediaSnapshot {
                    current_track,
                    playing: player.is_playing(),
                    playlist_len: playlist.len(),
                    playlist_idx,
                });

                thread::sleep(Duration::from_millis(150));
            }
        });
    }



    fn apply_m3_visuals(palette: &M3Palette, ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();

        visuals.window_fill = Color32::TRANSPARENT;
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.extreme_bg_color = Color32::TRANSPARENT;
        visuals.button_frame = true;

        let _surface = Color32::from_rgb(palette.surface[0], palette.surface[1], palette.surface[2]);
        let on_surface = Color32::from_rgb(palette.on_surface[0], palette.on_surface[1], palette.on_surface[2]);
        let primary = Color32::from_rgb(palette.primary[0], palette.primary[1], palette.primary[2]);
        let on_primary = Color32::from_rgb(palette.on_primary[0], palette.on_primary[1], palette.on_primary[2]);
        let primary_container = Color32::from_rgb(palette.primary_container[0], palette.primary_container[1], palette.primary_container[2]);
        let on_primary_container = Color32::from_rgb(palette.on_primary_container[0], palette.on_primary_container[1], palette.on_primary_container[2]);
        let surface_variant = Color32::from_rgb(palette.surface_variant[0], palette.surface_variant[1], palette.surface_variant[2]);
        let _on_surface_variant = Color32::from_rgb(palette.on_surface_variant[0], palette.on_surface_variant[1], palette.on_surface_variant[2]);
        let outline = Color32::from_rgb(palette.outline[0], palette.outline[1], palette.outline[2]);
        let _secondary = Color32::from_rgb(palette.secondary[0], palette.secondary[1], palette.secondary[2]);

        visuals.widgets.noninteractive.bg_fill = surface_variant;
        visuals.widgets.noninteractive.fg_stroke.color = on_surface;
        visuals.widgets.noninteractive.weak_bg_fill = surface_variant;

        visuals.widgets.inactive.bg_fill = surface_variant;
        visuals.widgets.inactive.fg_stroke.color = on_surface;
        visuals.widgets.inactive.weak_bg_fill = surface_variant;
        visuals.widgets.inactive.bg_stroke.color = outline;

        visuals.widgets.hovered.bg_fill = primary_container;
        visuals.widgets.hovered.fg_stroke.color = on_primary_container;
        visuals.widgets.hovered.weak_bg_fill = primary_container;

        visuals.widgets.active.bg_fill = primary;
        visuals.widgets.active.fg_stroke.color = on_primary;
        visuals.widgets.active.weak_bg_fill = primary;

        visuals.widgets.active.fg_stroke.color = on_primary;
        visuals.widgets.hovered.fg_stroke.color = on_primary_container;

        visuals.selection.bg_fill = primary;

        visuals.override_text_color = Some(on_surface);

        ctx.set_visuals(visuals);
    }

    /// Current playing track from the player's playlist state.
    pub fn current_track(&self) -> Option<Track> {
        let pl = self.player.playlist();
        let idx = self.player.playlist_idx();
        pl.get(idx).cloned()
    }

    pub fn ensure_cover_loaded(&mut self, _ctx: &egui::Context, ovride: bool) {
        let cover = self.player.cover();
        let should_reload = ovride || self.previous_cover_data != cover.data;

        if should_reload {
            self.previous_cover_data = cover.data.clone();

            let cfg = self.config.lock().unwrap().clone();
            let cover_data = cover.data.clone();
            let cover_mime = cover.mime.clone();
            let current_track = self.current_track().map(|t| t.path);
            let is_default = is_default_cover(&cover);
            let result = self.pending_cover_result.clone();

            thread::spawn(move || {
                let color_image = image::load_from_memory(&cover_data).ok().map(|img| {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba)
                });

                let src = CoverArt { data: cover_data.clone(), mime: cover_mime };
                let palette = match cfg.theme.source {
                    ThemeSource::AlbumCover => {
                        let folder = is_default.then(|| {
                            current_track.as_ref()
                                .and_then(|p| find_folder_cover(p))
                                .and_then(|b| extract_palette_from_bytes(&b))
                        }).flatten();
                        folder.unwrap_or_else(|| extract_palette(src))
                    }
                    ThemeSource::SystemWallpaper => {
                        get_system_wallpaper_buffer()
                            .and_then(|b| extract_palette_from_bytes(&b))
                            .unwrap_or_else(|| extract_palette(src))
                    }
                    ThemeSource::Manual => cfg.theme.palette.clone(),
                };

                let mut lock = result.lock().unwrap();
                *lock = Some(CoverWorkResult { cover_data, color_image, palette });
            });
        }
    }

    pub fn apply_pending_cover(&mut self, ctx: &egui::Context) {
        let mut lock = self.pending_cover_result.lock().unwrap();
        if let Some(result) = lock.take() {
            self.previous_cover_data = result.cover_data;
            if let Some(image) = result.color_image {
                self.cover_texture = Some(ctx.load_texture("cover_art", image, Default::default()));
            }
            self.target_palette = result.palette.clone();
            self.text_color = Color32::from_rgb(
                result.palette.on_surface[0],
                result.palette.on_surface[1],
                result.palette.on_surface[2],
            );
            Self::apply_m3_visuals(&result.palette, ctx);
        }
    }

    pub fn load_library_async(&mut self) {
        if self.library_loading {
            return;
        }
        self.library_loading = true;
        let cfg = self.config.lock().unwrap();
        let player = self.player.clone();
        let dirs = cfg.music_dirs.clone();
        let sort_option = self.sort_option.clone();
        let startup_tracks = self.startup_tracks.clone();
        let should_autoplay = !startup_tracks.is_empty();

        std::thread::spawn(move || {
            let mut library_tracks = scan_music_dirs(&dirs);

            if matches!(sort_option, Options::Alphabetical) {
                library_tracks.sort_by(|a, b| a.title.cmp(&b.title));
            }

            let mut tracks = Vec::with_capacity(startup_tracks.len() + library_tracks.len());
            let mut seen_paths = HashSet::new();

            for track in startup_tracks.into_iter().chain(library_tracks.into_iter()) {
                if seen_paths.insert(track.path.clone()) {
                    tracks.push(track);
                }
            }

            let has_tracks = !tracks.is_empty();

            if has_tracks {
                if should_autoplay {
                    player.send(PlayerCommand::SetPlaylistAndPlayIndex(tracks, 0));
                } else {
                    player.send(PlayerCommand::SetPlaylist(tracks));
                }
            }
        });
    }
}
