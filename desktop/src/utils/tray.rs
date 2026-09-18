//! System tray icon (StatusNotifierItem) with playback controls.
//!
//!   [sync thread] --MediaSnapshot--> TrayHandle::sync --> ksni service
//!   menu / activate --PlayerCommand--> audio engine
//!                 \-> AppAction::Show/Quit --> main action loop
//!
//! Requires a StatusNotifierItem host (KDE, waybar, GNOME + AppIndicator…).
//! If none is available the service logs and the app keeps running without
//! the tray.

#[cfg(target_os = "linux")]
mod linux {
    use super::super::app_action::{AppAction, spawn_quit_watchdog};
    use crate::utils::media_controls::MediaSnapshot;
    use ksni::menu::{CheckmarkItem, Disposition, MenuItem, StandardItem};
    use ksni::{Category, Icon, OfflineReason, Orientation, Status, ToolTip, Tray};
    use player_core::{Player as CorePlayer, PlayerCommand};
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex, OnceLock};

    const TRAY_ICON: &[u8] = include_bytes!("../../../assets/reamped.png");

    fn tray_icons() -> &'static [Icon] {
        static ICONS: OnceLock<Vec<Icon>> = OnceLock::new();
        ICONS.get_or_init(|| {
            let img = image::load_from_memory(TRAY_ICON).expect("failed to decode tray icon");
            let rgba = img
                .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            let data = rgba
                .pixels()
                .flat_map(|p| [p[3], p[0], p[1], p[2]])
                .collect();
            vec![Icon {
                width: 32,
                height: 32,
                data,
            }]
        })
    }

    /// State pushed into the tray from the media sync thread.
    #[derive(Clone, PartialEq)]
    struct TrayState {
        has_track: bool,
        playing: bool,
        title: String,
        artist: String,
        shuffle: bool,
        repeat: bool,
        repeat_one: bool,
    }

    impl TrayState {
        fn from_snapshot(s: &MediaSnapshot) -> Self {
            let track = track_of(s);
            Self {
                has_track: track.is_some(),
                playing: s.playing,
                title: track.map(|t| t.title.clone()).unwrap_or_default(),
                artist: track.map(|t| t.artist.clone()).unwrap_or_default(),
                shuffle: s.shuffle,
                repeat: s.repeat,
                repeat_one: s.repeat_one,
            }
        }
    }

    fn track_of(s: &MediaSnapshot) -> Option<&player_core::Track> {
        s.current_track.as_ref()
    }

    struct ReAmpedTray {
        player: CorePlayer,
        actions: Sender<AppAction>,
        playing: bool,
        has_track: bool,
        title: String,
        artist: String,
        shuffle: bool,
        repeat: bool,
        repeat_one: bool,
    }

    impl ReAmpedTray {
        fn sync_from(&mut self, s: &MediaSnapshot) {
            self.playing = s.playing;
            self.has_track = s.current_track.is_some();
            let track = track_of(s);
            self.title = track.map(|t| t.title.clone()).unwrap_or_default();
            self.artist = track.map(|t| t.artist.clone()).unwrap_or_default();
            self.shuffle = s.shuffle;
            self.repeat = s.repeat;
            self.repeat_one = s.repeat_one;
        }

        fn quit(&mut self) {
            self.player.send(PlayerCommand::Stop);
            let _ = self.actions.send(AppAction::Quit);
            spawn_quit_watchdog();
        }
    }

    impl Tray for ReAmpedTray {
        fn id(&self) -> String {
            "ReAmped".into()
        }

        fn category(&self) -> Category {
            Category::ApplicationStatus
        }

        fn status(&self) -> Status {
            Status::Active
        }

        fn title(&self) -> String {
            "ReAmped".into()
        }

        fn icon_name(&self) -> String {
            "ReAmped".into()
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            tray_icons().to_vec()
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip {
                icon_name: String::new(),
                icon_pixmap: tray_icons().to_vec(),
                title: "ReAmped".into(),
                description: if self.has_track {
                    format!("{} - {}", self.artist, self.title)
                } else {
                    "Sin reproducción".into()
                },
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.actions.send(AppAction::Show);
        }

        fn scroll(&mut self, delta: i32, _orientation: Orientation) {
            let volume = (self.player.volume() + delta.signum() as f32 * 0.05).clamp(0.0, 1.0);
            self.player.send(PlayerCommand::SetVolume(volume));
        }

        fn watcher_offline(&self, reason: OfflineReason) -> bool {
            eprintln!("[Tray] StatusNotifierWatcher offline: {reason:?}");
            true
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                MenuItem::Standard(StandardItem {
                    label: "Mostrar ReAmped".into(),
                    enabled: true,
                    visible: true,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        let _ = t.actions.send(AppAction::Show);
                    }),
                }),
                MenuItem::Separator,
                MenuItem::Standard(StandardItem {
                    label: if self.playing {
                        "⏸ Pausar"
                    } else {
                        "▶ Reproducir"
                    }
                    .into(),
                    enabled: true,
                    visible: true,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(if t.playing {
                            PlayerCommand::Pause
                        } else {
                            PlayerCommand::Play
                        });
                    }),
                }),
                MenuItem::Standard(StandardItem {
                    label: "⏹ Detener".into(),
                    enabled: self.has_track,
                    visible: true,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(PlayerCommand::Stop);
                    }),
                }),
                MenuItem::Standard(StandardItem {
                    label: "⏮ Anterior".into(),
                    enabled: self.has_track,
                    visible: true,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(PlayerCommand::Prev);
                    }),
                }),
                MenuItem::Standard(StandardItem {
                    label: "⏭ Siguiente".into(),
                    enabled: self.has_track,
                    visible: true,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(PlayerCommand::Next);
                    }),
                }),
                MenuItem::Separator,
                MenuItem::Checkmark(CheckmarkItem {
                    label: "🔀 Aleatorio".into(),
                    enabled: true,
                    visible: true,
                    checked: self.shuffle,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(PlayerCommand::ToggleShuffle);
                    }),
                }),
                MenuItem::Checkmark(CheckmarkItem {
                    label: "🔁 Repetir todo".into(),
                    enabled: true,
                    visible: true,
                    checked: self.repeat,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(PlayerCommand::ToggleRepeat);
                    }),
                }),
                MenuItem::Checkmark(CheckmarkItem {
                    label: "🔂 Repetir una".into(),
                    enabled: true,
                    visible: true,
                    checked: self.repeat_one,
                    disposition: Disposition::Normal,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    activate: Box::new(|t: &mut Self| {
                        t.player.send(PlayerCommand::ToggleRepeatOne);
                    }),
                }),
                MenuItem::Separator,
                MenuItem::Standard(StandardItem {
                    label: "❌ Salir".into(),
                    enabled: true,
                    visible: true,
                    icon_name: String::new(),
                    icon_data: Vec::new(),
                    shortcut: Vec::new(),
                    disposition: Disposition::Normal,
                    activate: Box::new(|t: &mut Self| t.quit()),
                }),
            ]
        }
    }

    /// Clonable handle to keep the tray menu in sync with playback state.
    #[derive(Clone)]
    pub struct TrayHandle {
        handle: ksni::blocking::Handle<ReAmpedTray>,
        last: Arc<Mutex<Option<TrayState>>>,
    }

    impl TrayHandle {
        /// Pushes a snapshot into the tray, only when something changed.
        pub fn sync(&self, snapshot: &MediaSnapshot) {
            let state = TrayState::from_snapshot(snapshot);
            let mut last = self.last.lock().unwrap();
            if last.as_ref() == Some(&state) {
                return;
            }
            *last = Some(state);
            self.handle.update(|tray| tray.sync_from(snapshot));
        }
    }

    /// Spawns the StatusNotifierItem service in its own thread.
    ///
    /// Returns `None` when there is no tray host available.
    pub fn start(core_player: CorePlayer, actions: Sender<AppAction>) -> Option<TrayHandle> {
        let tray = ReAmpedTray {
            player: core_player,
            actions,
            playing: false,
            has_track: false,
            title: String::new(),
            artist: String::new(),
            shuffle: false,
            repeat: false,
            repeat_one: false,
        };

        use ksni::blocking::TrayMethods;
        match tray.spawn() {
            Ok(handle) => Some(TrayHandle {
                handle,
                last: Arc::new(Mutex::new(None)),
            }),
            Err(e) => {
                eprintln!("[Tray] no se pudo iniciar la bandeja: {e:?}");
                None
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod fallback {
    use super::super::app_action::AppAction;
    use crate::utils::media_controls::MediaSnapshot;
    use player_core::Player;
    use std::sync::mpsc::Sender;

    #[derive(Clone)]
    pub struct TrayHandle;

    impl TrayHandle {
        pub fn sync(&self, _snapshot: &MediaSnapshot) {}
    }

    pub fn start(_core_player: Player, _actions: Sender<AppAction>) -> Option<TrayHandle> {
        None
    }
}

#[cfg(target_os = "linux")]
pub use linux::{TrayHandle, start};

#[cfg(not(target_os = "linux"))]
pub use fallback::{TrayHandle, start};
