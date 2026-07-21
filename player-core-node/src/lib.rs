use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use napi::{
    threadsafe_function::{
        ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode,
    },
    JsFunction,
};
use napi_derive::napi;

use player_core::{PlayerBuilder, PlayerCommand};

fn event_to_json(event: &player_core::Event) -> String {
    match event {
        player_core::Event::StateChanged => {
            r#"{"kind":"StateChanged"}"#.to_string()
        }
        player_core::Event::TrackChanged(idx) => {
            format!(r#"{{"kind":"TrackChanged","index":{}}}"#, idx)
        }
        player_core::Event::PlaylistChanged => {
            r#"{"kind":"PlaylistChanged"}"#.to_string()
        }
        player_core::Event::Loudness(l, r) => {
            format!(
                r#"{{"kind":"Loudness","left":{},"right":{}}}"#,
                l, r
            )
        }
        player_core::Event::Error(e) => {
            format!(r#"{{"kind":"Error","message":"{}"}}"#, e)
        }
        player_core::Event::Shutdown => {
            r#"{"kind":"Shutdown"}"#.to_string()
        }
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct JsTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
}

impl From<player_core::Track> for JsTrack {
    fn from(t: player_core::Track) -> Self {
        JsTrack {
            path: t.path.to_string_lossy().to_string(),
            title: t.title,
            artist: t.artist,
            duration: t.duration as f64,
        }
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct JsMetadata {
    pub title: String,
    pub artist: String,
    pub duration: f64,
    pub cover: Vec<u8>,
}

#[napi]
pub struct JsPlayer {
    inner: player_core::Player,
    shutdown: Arc<AtomicBool>,
}

#[napi]
impl JsPlayer {
    #[napi(constructor)]
    pub fn new(volume: f64) -> Self {
        let player = PlayerBuilder::new().with_volume(volume as f32).build();
        JsPlayer {
            inner: player,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    // -- Transport controls --

    #[napi]
    pub fn play(&self) {
        self.inner.send(PlayerCommand::Play);
    }

    #[napi]
    pub fn pause(&self) {
        self.inner.send(PlayerCommand::Pause);
    }

    #[napi]
    pub fn toggle_play(&self) {
        if self.inner.is_playing() {
            self.inner.send(PlayerCommand::Pause);
        } else {
            self.inner.send(PlayerCommand::Play);
        }
    }

    #[napi]
    pub fn stop(&self) {
        self.inner.send(PlayerCommand::Stop);
    }

    #[napi]
    pub fn next(&self) {
        self.inner.send(PlayerCommand::Next);
    }

    #[napi]
    pub fn prev(&self) {
        self.inner.send(PlayerCommand::Prev);
    }

    // -- Settings --

    #[napi]
    pub fn set_volume(&self, vol: f64) {
        self.inner.send(PlayerCommand::SetVolume(vol as f32));
    }

    #[napi]
    pub fn seek(&self, pos: f64) {
        self.inner.send(PlayerCommand::Seek(pos as f32));
    }

    #[napi]
    pub fn toggle_shuffle(&self) {
        self.inner.send(PlayerCommand::ToggleShuffle);
    }

    #[napi]
    pub fn toggle_repeat(&self) {
        self.inner.send(PlayerCommand::ToggleRepeat);
    }

    #[napi]
    pub fn toggle_repeat_one(&self) {
        self.inner.send(PlayerCommand::ToggleRepeatOne);
    }

    #[napi]
    pub fn set_eq_bass(&self, gain: f64) {
        self.inner.send(PlayerCommand::SetGainBass(gain as f32));
    }

    #[napi]
    pub fn set_eq_mid(&self, gain: f64) {
        self.inner.send(PlayerCommand::SetGainMid(gain as f32));
    }

    #[napi]
    pub fn set_eq_high(&self, gain: f64) {
        self.inner.send(PlayerCommand::SetGainHigh(gain as f32));
    }

    #[napi]
    pub fn set_expander_width(&self, width: f64) {
        self.inner.send(PlayerCommand::SetExpanderWidth(width as f32));
    }

    // -- Playlist --

    #[napi]
    pub fn set_playlist(&self, paths: Vec<String>) {
        let tracks: Vec<player_core::Track> = paths
            .into_iter()
            .filter_map(|p| {
                let path = std::path::PathBuf::from(&p);
                if path.exists() {
                    player_core::metadata::read_metadata(&path).map(|m| {
                        player_core::Track {
                            path,
                            title: m.title,
                            artist: m.artist,
                            duration: m.duration,
                        }
                    })
                } else {
                    None
                }
            })
            .collect();
        self.inner.send(PlayerCommand::SetPlaylist(tracks));
    }

    #[napi]
    pub fn play_index(&self, index: i32) {
        self.inner.send(PlayerCommand::PlayIndex(index as usize));
    }

    #[napi]
    pub fn set_playlist_and_play_index(&self, paths: Vec<String>, index: i32) {
        let tracks: Vec<player_core::Track> = paths
            .into_iter()
            .filter_map(|p| {
                let path = std::path::PathBuf::from(&p);
                if path.exists() {
                    player_core::metadata::read_metadata(&path).map(|m| {
                        player_core::Track {
                            path,
                            title: m.title,
                            artist: m.artist,
                            duration: m.duration,
                        }
                    })
                } else {
                    None
                }
            })
            .collect();
        self.inner
            .send(PlayerCommand::SetPlaylistAndPlayIndex(tracks, index as usize));
    }

    #[napi]
    pub fn jump_to(&self, index: i32) {
        self.inner.send(PlayerCommand::JumpTo(index as usize));
    }

    #[napi]
    pub fn shuffle_playlist(&self) {
        self.inner.send(PlayerCommand::AleatoryFullRandom);
    }

    // -- Getters --

    #[napi]
    pub fn is_playing(&self) -> bool {
        self.inner.is_playing()
    }

    #[napi]
    pub fn position(&self) -> f64 {
        self.inner.position() as f64
    }

    #[napi]
    pub fn duration(&self) -> f64 {
        self.inner.duration() as f64
    }

    #[napi]
    pub fn volume(&self) -> f64 {
        self.inner.volume() as f64
    }

    #[napi]
    pub fn shuffle(&self) -> bool {
        self.inner.shuffle()
    }

    #[napi]
    pub fn repeat(&self) -> bool {
        self.inner.repeat()
    }

    #[napi]
    pub fn repeat_one(&self) -> bool {
        self.inner.repeat_one()
    }

    #[napi]
    pub fn get_loudness(&self) -> Vec<f64> {
        let (l, r) = self.inner.get_loudness();
        vec![l as f64, r as f64]
    }

    #[napi]
    pub fn get_sample_rate(&self) -> f64 {
        self.inner.get_sample_rate() as f64
    }

    #[napi]
    pub fn playlist_length(&self) -> i32 {
        self.inner.playlist().len() as i32
    }

    #[napi]
    pub fn playlist_index(&self) -> i32 {
        self.inner.playlist_idx() as i32
    }

    #[napi]
    pub fn cover(&self) -> Vec<u8> {
        self.inner.cover().data
    }

    #[napi]
    pub fn metadata(&self) -> Option<JsMetadata> {
        self.inner.metadata().map(|m| JsMetadata {
            title: m.title,
            artist: m.artist,
            duration: m.duration as f64,
            cover: m.cover.data,
        })
    }

    // -- Events --

    #[napi]
    pub fn set_event_callback(&self, callback: JsFunction) -> napi::Result<()> {
        let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = callback
            .create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;

        let player = self.inner.clone();
        let shutdown = self.shutdown.clone();

        std::thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                if let Some(event) = player.try_recv_event() {
                    let json = event_to_json(&event);
                    if tsfn.call(json, ThreadsafeFunctionCallMode::NonBlocking)
                        != napi::Status::Ok
                    {
                        break;
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });

        Ok(())
    }

    #[napi]
    pub fn poll_event(&self) -> Option<String> {
        self.inner.try_recv_event().as_ref().map(event_to_json)
    }

    // -- Cleanup --

    #[napi]
    pub fn destroy(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

impl Drop for JsPlayer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
