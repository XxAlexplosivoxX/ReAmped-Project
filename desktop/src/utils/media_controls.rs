use player_core::Track;

/// Window-level action requested through the MPRIS root interface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MprisAction {
    Raise,
    Quit,
}

/// Snapshot of the playback state pushed to the MPRIS service.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSnapshot {
    pub current_track: Option<Track>,
    pub playing: bool,
    pub playlist_len: usize,
    pub playlist_idx: usize,
    pub position: f32,
    pub duration: f32,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: bool,
    pub repeat_one: bool,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{MediaSnapshot, MprisAction};
    use crate::utils::misc::find_folder_cover;
    use async_channel::{Sender, unbounded};
    use async_std::task;
    use mpris_server::{
        LoopStatus, Metadata as MprisMetadata, PlaybackStatus, Player as MprisPlayer, Time,
        TrackId, Volume,
    };
    use player_core::metadata::is_default_cover;
    use player_core::{Player as CorePlayer, PlayerCommand, Track};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc as std_mpsc;
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::Instant,
    };

    #[derive(Clone)]
    pub struct MediaControls {
        tx: Sender<MediaEvent>,
        last_snapshot: Arc<Mutex<Option<MediaSnapshot>>>,
        action_rx: Arc<Mutex<std_mpsc::Receiver<MprisAction>>>,
    }

    enum MediaEvent {
        Sync(MediaSnapshot),
    }

    /// Capabilities exposed on the MPRIS player interface.
    #[derive(Clone, Copy, PartialEq)]
    struct Caps {
        play: bool,
        pause: bool,
        next: bool,
        prev: bool,
        seek: bool,
    }

    /// Locally tracked state used to avoid redundant D-Bus property updates
    /// and to detect external seeks.
    #[derive(Default)]
    struct SyncState {
        track_path: Option<PathBuf>,
        cover_hash: u64,
        cover_file: Option<PathBuf>,
        status: Option<PlaybackStatus>,
        caps: Option<Caps>,
        loop_status: Option<LoopStatus>,
        shuffle: Option<bool>,
        position: f32,
        stamp: Option<Instant>,
        hold_seeked: bool,
    }

    impl SyncState {
        async fn apply(
            &mut self,
            player: &MprisPlayer,
            core: &CorePlayer,
            s: &MediaSnapshot,
            applied_volume: &Mutex<f32>,
        ) {
            let has_track = s.current_track.is_some();
            let status = if has_track && s.playing {
                PlaybackStatus::Playing
            } else if has_track {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Stopped
            };
            if self.status != Some(status) {
                let _ = player.set_playback_status(status).await;
                self.status = Some(status);
            }

            let caps = Caps {
                play: has_track,
                pause: has_track,
                next: has_track && (s.playlist_idx + 1 < s.playlist_len || s.repeat),
                prev: has_track && (s.playlist_idx > 0 || s.repeat),
                seek: has_track,
            };
            if self.caps != Some(caps) {
                let _ = player.set_can_play(caps.play).await;
                let _ = player.set_can_pause(caps.pause).await;
                let _ = player.set_can_go_next(caps.next).await;
                let _ = player.set_can_go_previous(caps.prev).await;
                let _ = player.set_can_seek(caps.seek).await;
                self.caps = Some(caps);
            }

            let track_path = s.current_track.as_ref().map(|t| t.path.clone());
            if track_path != self.track_path {
                self.track_path = track_path;
                self.hold_seeked = true;
                self.publish_track(player, core, s.current_track.clone(), s.duration)
                    .await;
            }

            let expected = match self.stamp {
                Some(t) if self.status == Some(PlaybackStatus::Playing) => {
                    self.position + t.elapsed().as_secs_f32()
                }
                Some(_) => self.position,
                None => s.position,
            };
            if !self.hold_seeked && (s.position - expected).abs() > 1.0 {
                let _ = player.seeked(to_time(s.position)).await;
            }
            self.hold_seeked = false;
            player.set_position(to_time(s.position));
            self.position = s.position;
            self.stamp = Some(Instant::now());

            if (s.volume - *applied_volume.lock().unwrap()).abs() > 0.005 {
                let _ = player.set_volume(s.volume.clamp(0.0, 1.0) as Volume).await;
                *applied_volume.lock().unwrap() = s.volume;
            }

            let loop_status = if s.repeat_one {
                LoopStatus::Track
            } else if s.repeat {
                LoopStatus::Playlist
            } else {
                LoopStatus::None
            };
            if self.loop_status != Some(loop_status) {
                let _ = player.set_loop_status(loop_status).await;
                self.loop_status = Some(loop_status);
            }

            if self.shuffle != Some(s.shuffle) {
                let _ = player.set_shuffle(s.shuffle).await;
                self.shuffle = Some(s.shuffle);
            }
        }

        async fn publish_track(
            &mut self,
            player: &MprisPlayer,
            core: &CorePlayer,
            track: Option<Track>,
            engine_duration: f32,
        ) {
            let Some(track) = track else {
                let _ = player.set_metadata(MprisMetadata::new()).await;
                player.set_position(Time::ZERO);
                return;
            };

            let track_id = track_id_for(&track.path);
            let art_url = self.cover_uri(core, &track.path);
            let length = if engine_duration > 0.0 {
                engine_duration
            } else {
                track.duration.max(0.0)
            };

            let mut builder = MprisMetadata::builder()
                .trackid(track_id)
                .title(track.title.clone())
                .artist([track.artist.clone()])
                .length(to_time(length))
                .url(file_uri(&track.path));
            if !track.album.is_empty() {
                builder = builder.album(track.album.clone());
            }
            if let Some(art) = art_url {
                builder = builder.art_url(art);
            }
            let _ = player.set_metadata(builder.build()).await;
            player.set_position(Time::ZERO);
        }

        /// Resolves the current cover art (embedded, falling back to a folder
        /// image), caches it on disk and returns its `file://` URI.
        fn cover_uri(&mut self, core: &CorePlayer, track_path: &Path) -> Option<String> {
            let cover = core.cover();
            let (data, mime) = if is_default_cover(&cover) {
                let data = find_folder_cover(track_path)?;
                let mime = infer::get(&data)
                    .map(|k| k.mime_type().to_string())
                    .unwrap_or_else(|| "image/jpeg".to_string());
                (data, mime)
            } else {
                (cover.data.clone(), cover.mime.as_str().to_string())
            };
            if data.is_empty() {
                return None;
            }

            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let hash = hasher.finish();
            if hash != self.cover_hash || self.cover_file.is_none() {
                self.cover_file = write_cover_cache(&data, &mime);
                self.cover_hash = hash;
            }
            self.cover_file.as_ref().map(|p| file_uri(p))
        }
    }

    impl MediaControls {
        pub fn start(core_player: CorePlayer) -> Self {
            let (tx, rx) = unbounded::<MediaEvent>();
            let (action_tx, action_rx) = std_mpsc::channel::<MprisAction>();
            let applied_volume = Arc::new(Mutex::new(-1.0));

            thread::spawn(move || {
                task::block_on(async move {
                    let player = MprisPlayer::builder("ReAmped")
                        .identity("ReAmped")
                        .desktop_entry("ReAmped")
                        .can_quit(true)
                        .can_raise(true)
                        .can_control(true)
                        .can_play(true)
                        .can_pause(true)
                        .can_seek(true)
                        .build()
                        .await
                        .expect("failed to start MPRIS service");

                    player.connect_raise({
                        let action_tx = action_tx.clone();
                        move |_| {
                            let _ = action_tx.send(MprisAction::Raise);
                        }
                    });
                    player.connect_quit({
                        let action_tx = action_tx.clone();
                        move |_| {
                            let _ = action_tx.send(MprisAction::Quit);
                        }
                    });
                    player.connect_play({
                        let core_player = core_player.clone();
                        move |_| {
                            core_player.send(PlayerCommand::Play);
                        }
                    });
                    player.connect_pause({
                        let core_player = core_player.clone();
                        move |_| {
                            core_player.send(PlayerCommand::Pause);
                        }
                    });
                    player.connect_play_pause({
                        let core_player = core_player.clone();
                        move |player| {
                            if core_player.is_playing()
                                || player.playback_status() == PlaybackStatus::Playing
                            {
                                core_player.send(PlayerCommand::Pause);
                            } else {
                                core_player.send(PlayerCommand::Play);
                            }
                        }
                    });
                    player.connect_next({
                        let core_player = core_player.clone();
                        move |_| {
                            core_player.send(PlayerCommand::Next);
                        }
                    });
                    player.connect_previous({
                        let core_player = core_player.clone();
                        move |_| {
                            core_player.send(PlayerCommand::Prev);
                        }
                    });
                    player.connect_stop({
                        let core_player = core_player.clone();
                        move |_| {
                            core_player.send(PlayerCommand::Stop);
                        }
                    });
                    player.connect_seek({
                        let core_player = core_player.clone();
                        move |_, delta| {
                            let target =
                                core_player.position() + delta.as_micros() as f32 / 1_000_000.0;
                            core_player.send(PlayerCommand::Seek(target.max(0.0)));
                        }
                    });
                    player.connect_set_position({
                        let core_player = core_player.clone();
                        move |mpris, track_id, position| {
                            if mpris.metadata().trackid().as_ref() == Some(track_id) {
                                core_player.send(PlayerCommand::Seek(
                                    position.as_micros() as f32 / 1_000_000.0,
                                ));
                            }
                        }
                    });
                    player.connect_set_volume({
                        let core_player = core_player.clone();
                        let applied_volume = applied_volume.clone();
                        move |_, volume| {
                            *applied_volume.lock().unwrap() = volume as f32;
                            core_player.send(PlayerCommand::SetVolume(volume as f32));
                        }
                    });
                    player.connect_set_loop_status({
                        let core_player = core_player.clone();
                        move |_, status| {
                            for cmd in loop_status_commands(&core_player, status) {
                                core_player.send(cmd);
                            }
                        }
                    });
                    player.connect_set_shuffle({
                        let core_player = core_player.clone();
                        move |_, shuffle| {
                            if core_player.shuffle() != shuffle {
                                core_player.send(PlayerCommand::ToggleShuffle);
                            }
                        }
                    });

                    task::spawn_local(player.run());

                    let mut sync = SyncState::default();
                    while let Ok(event) = rx.recv().await {
                        let MediaEvent::Sync(snapshot) = event;
                        sync.apply(&player, &core_player, &snapshot, &applied_volume)
                            .await;
                    }
                });
            });

            Self {
                tx,
                last_snapshot: Arc::new(Mutex::new(None)),
                action_rx: Arc::new(Mutex::new(action_rx)),
            }
        }

        pub fn sync_from_snapshot(&self, snapshot: MediaSnapshot) {
            let mut last_snapshot = self.last_snapshot.lock().unwrap();
            if last_snapshot.as_ref() == Some(&snapshot) {
                return;
            }

            *last_snapshot = Some(snapshot.clone());
            let _ = self.tx.try_send(MediaEvent::Sync(snapshot));
        }

        /// Pops a pending window action requested through MPRIS.
        pub fn poll_action(&self) -> Option<MprisAction> {
            self.action_rx.lock().unwrap().try_recv().ok()
        }
    }

    fn loop_status_commands(core: &CorePlayer, status: LoopStatus) -> Vec<PlayerCommand> {
        let (want_repeat, want_one) = match status {
            LoopStatus::None => (false, false),
            LoopStatus::Playlist => (true, false),
            LoopStatus::Track => (false, true),
        };

        let mut cmds = Vec::new();
        if core.repeat_one() && !want_one {
            cmds.push(PlayerCommand::ToggleRepeatOne);
        }
        if core.repeat() != want_repeat {
            cmds.push(PlayerCommand::ToggleRepeat);
        }
        if want_one && !core.repeat_one() {
            cmds.push(PlayerCommand::ToggleRepeatOne);
        }
        cmds
    }

    fn to_time(secs: f32) -> Time {
        Time::from_micros((secs * 1_000_000.0) as i64)
    }

    fn track_id_for(path: &Path) -> TrackId {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        TrackId::try_from(format!("/io/github/reamped/track/{:016x}", hasher.finish()))
            .unwrap_or(TrackId::NO_TRACK)
    }

    fn write_cover_cache(data: &[u8], mime: &str) -> Option<PathBuf> {
        let ext = if mime.contains("png") { "png" } else { "jpg" };
        let dir = dirs_next::cache_dir()?.join("reamped");
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("cover.{ext}"));
        std::fs::write(&path, data).ok()?;
        Some(path)
    }

    fn file_uri(path: &Path) -> String {
        let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut uri = String::from("file://");
        for byte in abs.to_string_lossy().as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                    uri.push(*byte as char)
                }
                _ => uri.push_str(&format!("%{byte:02X}")),
            }
        }
        uri
    }
}

#[cfg(not(target_os = "linux"))]
mod fallback {
    use super::{MediaSnapshot, MprisAction};

    #[derive(Clone)]
    pub struct MediaControls;

    impl MediaControls {
        pub fn start(_core_player: player_core::Player) -> Self {
            Self
        }

        pub fn sync_from_snapshot(&self, _snapshot: MediaSnapshot) {}

        pub fn poll_action(&self) -> Option<MprisAction> {
            None
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::MediaControls;

#[cfg(not(target_os = "linux"))]
pub use fallback::MediaControls;
