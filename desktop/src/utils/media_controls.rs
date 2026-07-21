#[cfg(target_os = "linux")]
mod linux {
    use async_channel::{Sender, unbounded};
    use async_std::task;
    use mpris_server::{Metadata as MprisMetadata, PlaybackStatus, Player as MprisPlayer, Time};
    use player_core::{Player as CorePlayer, PlayerCommand, Track};
    use std::{sync::{Arc, Mutex}, thread};

    #[derive(Clone, Debug, PartialEq)]
    pub struct MediaSnapshot {
        pub current_track: Option<Track>,
        pub playing: bool,
        pub playlist_len: usize,
        pub playlist_idx: usize,
    }

    #[derive(Clone)]
    pub struct MediaControls {
        tx: Sender<MediaEvent>,
        last_snapshot: Arc<Mutex<Option<MediaSnapshot>>>,
    }

    enum MediaEvent {
        Sync(MediaSnapshot),
    }

    impl MediaControls {
        pub fn start(core_player: CorePlayer) -> Self {
            let (tx, rx) = unbounded::<MediaEvent>();

            thread::spawn(move || {
                task::block_on(async move {
                    let player = MprisPlayer::builder("ReAmped")
                        .identity("ReAmped")
                        .desktop_entry("ReAmped")
                        .can_control(true)
                        .can_play(true)
                        .can_pause(true)
                        .can_go_next(false)
                        .can_go_previous(false)
                        .build()
                        .await
                        .expect("failed to start MPRIS service");

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
                            if core_player.is_playing() || player.playback_status() == PlaybackStatus::Playing {
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

                    task::spawn_local(player.run());

                    while let Ok(event) = rx.recv().await {
                        match event {
                            MediaEvent::Sync(snapshot) => sync_player(&player, snapshot).await,
                        }
                    }
                });
            });

            Self {
                tx,
                last_snapshot: Arc::new(Mutex::new(None)),
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
    }

    async fn sync_player(player: &MprisPlayer, snapshot: MediaSnapshot) {
        let has_track = snapshot.current_track.is_some();
        let playback_status = if has_track && snapshot.playing {
            PlaybackStatus::Playing
        } else if has_track {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Stopped
        };

        let _ = player.set_playback_status(playback_status).await;
        let _ = player.set_can_play(has_track).await;
        let _ = player.set_can_pause(has_track).await;
        let _ = player
            .set_can_go_next(snapshot.playlist_idx + 1 < snapshot.playlist_len)
            .await;
        let _ = player.set_can_go_previous(snapshot.playlist_idx > 0).await;

        if let Some(track) = snapshot.current_track {
            let metadata = MprisMetadata::builder()
                .title(track.title)
                .artist([track.artist])
                .length(Time::from_micros((track.duration.max(0.0) * 1_000_000.0) as i64))
                .build();

            let _ = player.set_metadata(metadata).await;
            player.set_position(Time::from_micros(0));
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod fallback {
    use player_core::Track;

    #[derive(Clone, Debug, PartialEq)]
    pub struct MediaSnapshot {
        pub current_track: Option<Track>,
        pub playing: bool,
        pub playlist_len: usize,
        pub playlist_idx: usize,
    }

    #[derive(Clone)]
    pub struct MediaControls;

    impl MediaControls {
        pub fn start(_core_player: player_core::Player) -> Self {
            Self
        }

        pub fn sync_from_snapshot(&self, _snapshot: MediaSnapshot) {}
    }
}

#[cfg(target_os = "linux")]
pub use linux::{MediaControls, MediaSnapshot};

#[cfg(not(target_os = "linux"))]
pub use fallback::{MediaControls, MediaSnapshot};