//! Application state and initialisation.
//!
//! # Architecture
//!
//! The [`PlayerApp`] struct is the top-level egui state.  It holds two logically
//! separate concerns:
//!
//! * **Player-core handle** (`Player`) — a cloneable handle to the audio engine.
//!   The player-core runs audio processing and decoding in **its own thread**,
//!   completely independent of the UI thread.
//! * **Egui state** — everything needed to render the UI: current volume,
//!   spectrum visualiser, album‑art texture, colour palette derived from the
//!   cover art, EQ/graphic‑equaliser values, a search string, and so on.
//!
//! # Thread model
//!
//! | Thread | Responsibility |
//! |---|---|
//! | **UI (eframe main thread)** | Runs `update()` every ~16 ms.  Paints egui widgets, polls player state, sends commands. |
//! | **Player‑core thread** | Decodes audio, drives the output device, fills shared sample buffers. |
//! | **Media‑sync thread** (spawned in [`PlayerApp::new`]) | Polls `player.playlist()`, `player.is_playing()` etc. every 150 ms and pushes snapshots into [`MediaControls`] for the UI to consume. |
//! | **Library‑scan thread** (spawned in [`PlayerApp::load_library_async`]) | Scans configured music directories on disk and sends a [`PlayerCommand::SetPlaylist`] (or `SetPlaylistAndPlayIndex`) when done. |
//!
//! # Communication patterns
//!
//! * **Commands** — the UI sends a `PlayerCommand` via `player.send(...)`.
//! * **State polling** — the UI reads `player.position()`, `player.is_playing()`,
//!   `player.duration()`, `player.samples()` etc. **every frame** (lock‑free
//!   shared state behind the handle).
//! * **Events** — `player.try_recv_event()` can be used to receive one‑shot
//!   notifications from the core (playback ended, track changed, …).
//!
//! The sub-module [`super::update`] contains the [`eframe::App`] implementation.

use std::{collections::HashSet, sync::{Arc, Mutex}, thread, time::Duration};
use egui::Color32;
use player_core::{
    Player, PlayerBuilder, PlayerCommand, Track, Options,
};
use crate::{utils::{load_cover::load_cover_texture, media_controls::{MediaControls, MediaSnapshot}, misc::extract_palette, luminance::luminance, scan_music_dirs::scan_music_dirs, visualizer::SpectrumVisualizer}};
use player_core::config::{AppConfig, load_config};

/// Top-level egui application state.
///
/// Every field is `pub` so that the sub‑components in [`crate::ui_elements`]
/// and [`crate::dsp_ui`] can read and write it directly (no separate
/// controller layer).
#[derive(Clone)]
pub struct PlayerApp {
    /// Cloneable handle to the player-core audio engine.
    pub player: Player,
    /// Current volume level (0.0 – 1.0).
    pub volume: f32,
    /// Spectrum / FFT visualiser state.
    pub visualizer: SpectrumVisualizer,
    /// Stateful media-control buttons (play/pause/next/prev).
    pub media_controls: MediaControls,
    /// Texture handle for the album-art cover, if loaded.
    pub cover_texture: Option<egui::TextureHandle>,
    /// The track currently shown in the UI (may differ from the core's active
    /// track while a seek or crossfade is in progress).
    pub current_track: Option<Track>,
    /// Cached playback position in seconds, updated every frame.
    pub position: f32,
    /// 3‑colour palette sorted by luminance.
    pub palette_sorted: Vec<[u8; 3]>,
    /// Target palette that `palette_sorted` lerps toward (crossfade transition).
    pub target_palette_sorted: Vec<[u8; 3]>,
    /// Human‑readable status string shown in the UI (e.g. `"status: Playing"`).
    pub state: &'static str,
    /// Text colour derived from the palette (contrasting with the background).
    pub text_color: Color32,
    /// Whether the window should be fullscreen.
    pub fullscreen: bool,
    /// Whether the configuration/settings window is visible.
    pub show_settings: bool,
    /// True during the very first frame after startup; used to suppress certain
    /// animations or transitions.
    pub just_executed: bool,
    /// Shared application configuration (loaded from disk on startup).
    pub config: Arc<Mutex<AppConfig>>,
    /// Custom background colour 1 (0.0 – 1.0).
    pub rgb1: [f32; 3],
    /// Custom background colour 2 (0.0 – 1.0).
    pub rgb2: [f32; 3],
    /// Custom background colour 3 (0.0 – 1.0).
    pub rgb3: [f32; 3],
    /// Whether colour‑picker 1 is expanded.
    pub show_picker1: bool,
    /// Whether colour‑picker 2 is expanded.
    pub show_picker2: bool,
    /// Whether colour‑picker 3 is expanded.
    pub show_picker3: bool,
    /// Current library search / filter string.
    pub search_str: String,
    /// Playlist sort order (normal or alphabetical).
    pub sort_option: Options,
    /// Bass equaliser gain.
    pub bass_val: f32,
    /// Mid equaliser gain.
    pub mid_val: f32,
    /// High equaliser gain.
    pub high_val: f32,
    /// Stereo‑width value.
    pub width_val: f32,
    /// Tracks passed via CLI arguments at startup.
    pub startup_tracks: Vec<Track>,
    /// Whether the playlist should scroll to the currently-playing track.
    pub scroll_current_track: bool,
    /// Accumulated time (seconds) for background gradient animation.
    pub bg_anim_t: f32,
    /// Cached marquee text (to avoid font shaping every frame).
    pub marquee_cache_text: String,
    /// Cached marquee galley (reused when text hasn't changed).
    pub marquee_cache_galley: Option<std::sync::Arc<egui::Galley>>,
    /// Available width when `marquee_cache_galley` was last laid out.
    pub marquee_cache_width: f32,
}

impl Default for PlayerApp {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl PlayerApp {
    /// Construct a new [`PlayerApp`].
    ///
    /// This constructor:
    /// 1. Loads the persisted [`AppConfig`] from disk.
    /// 2. Builds a [`Player`] via [`PlayerBuilder`], seeded with the saved
    ///    volume.
    /// 3. Creates a [`SpectrumVisualizer`] and a [`MediaControls`] handle.
    /// 4. Initialises every UI field to its default / config-derived value.
    /// 5. Spawns a **media‑sync background thread** that regularly copies the
    ///    core's playlist snapshot into `media_controls`.
    ///
    /// `startup_tracks` — tracks discovered from CLI arguments; they will be
    /// merged with the library scan result in [`load_library_async`](Self::load_library_async).
    pub fn new(startup_tracks: Vec<Track>) -> Self {
        let config_values = load_config();
        let config = Arc::new(Mutex::new(config_values.clone()));
        let visualizer = SpectrumVisualizer::new(config.clone());
        let player = PlayerBuilder::new().with_volume(config_values.volume).build();
        let media_controls = MediaControls::start(player.clone());

        let app = Self {
            player,
            volume: config_values.volume,
            visualizer,
            media_controls,
            cover_texture: None,
            current_track: None,
            position: 0.0,
            palette_sorted: vec![[0, 0, 0], [0, 0, 0], [0, 0, 0]],
            target_palette_sorted: vec![[0, 0, 0], [0, 0, 0], [0, 0, 0]],
            state: "status: Welcome",
            text_color: Color32::WHITE,
            fullscreen: config_values.fullscreen,
            show_settings: false,
            just_executed: true,
            config,
            rgb1: [
                config_values.theme.pallete_custom[0][0] as f32 / 255.0,
                config_values.theme.pallete_custom[0][1] as f32 / 255.0,
                config_values.theme.pallete_custom[0][2] as f32 / 255.0,
            ],
            rgb2: [
                config_values.theme.pallete_custom[1][0] as f32 / 255.0,
                config_values.theme.pallete_custom[1][1] as f32 / 255.0,
                config_values.theme.pallete_custom[1][2] as f32 / 255.0,
            ],
            rgb3: [
                config_values.theme.pallete_custom[2][0] as f32 / 255.0,
                config_values.theme.pallete_custom[2][1] as f32 / 255.0,
                config_values.theme.pallete_custom[2][2] as f32 / 255.0,
            ],
            show_picker1: false,
            show_picker2: false,
            show_picker3: false,
            search_str: String::from(""),
            sort_option: Options::Normal,
            bass_val: 1.0,
            mid_val: 1.0,
            high_val: 1.0,
            width_val: 1.0,
            startup_tracks,
            scroll_current_track: false,
            bg_anim_t: 0.0,
            marquee_cache_text: String::new(),
            marquee_cache_galley: None,
            marquee_cache_width: 0.0,
        };

        app.spawn_media_sync_thread();
        app
    }

    /// Spawns a background thread that periodically synchronises the core's
    /// playlist state into [`MediaControls`].
    ///
    /// The thread runs an infinite loop with a 150 ms sleep between iterations.
    /// Each tick it reads `player.playlist()`, `player.playlist_idx()`, and
    /// `player.is_playing()`, then pushes a [`MediaSnapshot`] to the
    /// `media_controls` channel.
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

    /// Load (or reload) the album‑art cover texture and derive a colour palette.
    ///
    /// If `ovride` is true, or the track has changed since the last call (or no
    /// cover is loaded yet), the method:
    ///
    /// 1. Fetches the cover bitmap from `self.player.cover()`.
    /// 2. Uploads it as an egui texture via [`load_cover_texture`].
    /// 3. Extracts a 3‑colour palette with [`extract_palette`].
    /// 4. Sorts the palette by luminance.
    /// 5. Rebuilds the [`egui::Visuals`] so that every widget colour derives
    ///    from the cover's dominant hues.
    ///
    /// If `cfg.theme.follow_cover` is false, the custom palette from
    /// configuration is used instead.
    pub fn ensure_cover_loaded(&mut self, ctx: &egui::Context, ovride: bool) {
        let cfg = self.config.lock().unwrap();
        let current_track = {
            let pl = self.player.playlist();
            let idx = self.player.playlist_idx();

            let has_loaded = self.player.duration() > 0.0
                || self.player.metadata().is_some()
                || self.player.is_playing();

            if has_loaded && !pl.is_empty() {
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
                self.target_palette_sorted = extract_palette(cover);
                self.target_palette_sorted
                    .sort_by(|a, b| luminance(*a).partial_cmp(&luminance(*b)).unwrap());
            } else {
                self.target_palette_sorted = cfg.theme.pallete_custom.clone();
                self.target_palette_sorted
                    .sort_by(|a, b| luminance(*a).partial_cmp(&luminance(*b)).unwrap());
            }
            let palette = self.target_palette_sorted.clone();
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

    /// Scan the configured music directories on a background thread.
    ///
    /// This method:
    /// 1. Locks the config to read `music_dirs`.
    /// 2. Spawns a `std::thread` that calls [`scan_music_dirs`].
    /// 3. Optionally sorts the result alphabetically.
    /// 4. Merges the library result with `self.startup_tracks`, deduplicating
    ///    by path.
    /// 5. Sends a [`PlayerCommand::SetPlaylist`] (or
    ///    `SetPlaylistAndPlayIndex` if startup tracks are present) to the
    ///    player core.
    ///
    /// This is called automatically when the playlist is empty (see the
    /// `update` method in [`super::update`]).
    pub fn load_library_async(&self) {
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