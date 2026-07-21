//! ReAmped Audio Player — Node.js bindings.
//!
//! This crate exposes the `player-core` engine to Node.js via N-API (napi-rs).
//! Provides a [`JsPlayer`] class with transport controls, EQ/DSP settings,
//! playlist management, event callbacks, and metadata access.
//!
//! # Events
//!
//! The player emits JSON-encoded events. Use [`JsPlayer::set_event_callback`]
//! to receive them asynchronously, or [`JsPlayer::poll_event`] for polling:
//!
//! ```json
//! {"kind":"StateChanged"}
//! {"kind":"TrackChanged","index":3}
//! {"kind":"PlaylistChanged"}
//! {"kind":"Loudness","left":0.85,"right":0.82}
//! {"kind":"Error","message":"..."}
//! {"kind":"Shutdown"}
//! ```

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

/// A track within the player's playlist.
///
/// Returned from playlist queries. Fields mirror the audio file's metadata.
#[napi(object)]
#[derive(Clone)]
pub struct JsTrack {
    /// Absolute file path on disk.
    pub path: String,
    /// Track title from metadata tags (or filename fallback).
    pub title: String,
    /// Artist name from metadata tags.
    pub artist: String,
    /// Duration in seconds.
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

/// Metadata for the currently loaded track.
#[napi(object)]
#[derive(Clone)]
pub struct JsMetadata {
    /// Track title.
    pub title: String,
    /// Artist name.
    pub artist: String,
    /// Duration in seconds.
    pub duration: f64,
    /// Raw cover art bytes (JPEG or PNG).
    pub cover: Vec<u8>,
}

/// High-performance audio player backed by the `player-core` engine.
///
/// Handles transport control, EQ/DSP, playlist management, loudness
/// metering, and event streaming. Create one via `new JsPlayer(volume?)`.
#[napi]
pub struct JsPlayer {
    inner: player_core::Player,
    shutdown: Arc<AtomicBool>,
}

#[napi]
impl JsPlayer {
    /// Create a new `JsPlayer` instance.
    ///
    /// `volume` — initial volume in the range `[0.0, 1.0]` (defaults to `1.0`).
    #[napi(constructor)]
    pub fn new(volume: f64) -> Self {
        let player = PlayerBuilder::new().with_volume(volume as f32).build();
        JsPlayer {
            inner: player,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    // -- Transport controls --

    /// Start or resume playback.
    #[napi]
    pub fn play(&self) {
        self.inner.send(PlayerCommand::Play);
    }

    /// Pause playback (position is preserved).
    #[napi]
    pub fn pause(&self) {
        self.inner.send(PlayerCommand::Pause);
    }

    /// Toggle between play and pause.
    #[napi]
    pub fn toggle_play(&self) {
        if self.inner.is_playing() {
            self.inner.send(PlayerCommand::Pause);
        } else {
            self.inner.send(PlayerCommand::Play);
        }
    }

    /// Stop playback and reset position.
    #[napi]
    pub fn stop(&self) {
        self.inner.send(PlayerCommand::Stop);
    }

    /// Skip to the next track in the playlist.
    #[napi]
    pub fn next(&self) {
        self.inner.send(PlayerCommand::Next);
    }

    /// Go back to the previous track.
    #[napi]
    pub fn prev(&self) {
        self.inner.send(PlayerCommand::Prev);
    }

    // -- Settings --

    /// Set the playback volume.
    ///
    /// `vol` — gain in the range `[0.0, 1.0]`.
    #[napi]
    pub fn set_volume(&self, vol: f64) {
        self.inner.send(PlayerCommand::SetVolume(vol as f32));
    }

    /// Seek to an absolute position in the current track.
    ///
    /// `pos` — position in seconds.
    #[napi]
    pub fn seek(&self, pos: f64) {
        self.inner.send(PlayerCommand::Seek(pos as f32));
    }

    /// Toggle shuffle mode on/off.
    #[napi]
    pub fn toggle_shuffle(&self) {
        self.inner.send(PlayerCommand::ToggleShuffle);
    }

    /// Toggle repeat-all mode on/off.
    #[napi]
    pub fn toggle_repeat(&self) {
        self.inner.send(PlayerCommand::ToggleRepeat);
    }

    /// Toggle repeat-one mode on/off.
    #[napi]
    pub fn toggle_repeat_one(&self) {
        self.inner.send(PlayerCommand::ToggleRepeatOne);
    }

    /// Set the bass shelf EQ gain.
    ///
    /// `gain` — gain in dB.
    #[napi]
    pub fn set_eq_bass(&self, gain: f64) {
        self.inner.send(PlayerCommand::SetGainBass(gain as f32));
    }

    /// Set the mid-band EQ gain.
    ///
    /// `gain` — gain in dB.
    #[napi]
    pub fn set_eq_mid(&self, gain: f64) {
        self.inner.send(PlayerCommand::SetGainMid(gain as f32));
    }

    /// Set the treble shelf EQ gain.
    ///
    /// `gain` — gain in dB.
    #[napi]
    pub fn set_eq_high(&self, gain: f64) {
        self.inner.send(PlayerCommand::SetGainHigh(gain as f32));
    }

    /// Set the stereo expander width.
    ///
    /// `width` — stereo width factor (`0.0` = mono, `1.0` = original).
    #[napi]
    pub fn set_expander_width(&self, width: f64) {
        self.inner.send(PlayerCommand::SetExpanderWidth(width as f32));
    }

    // -- Playlist --

    /// Replace the playlist with the given file paths.
    ///
    /// Only existing files with readable audio metadata are kept.
    /// `paths` — absolute or relative file paths.
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

    /// Play the track at `index` (current playlist).
    ///
    /// `index` — zero-based track index.
    #[napi]
    pub fn play_index(&self, index: i32) {
        self.inner.send(PlayerCommand::PlayIndex(index as usize));
    }

    /// Replace the playlist and immediately start playing at `index`.
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

    /// Jump to `index` in the playlist without restarting playback.
    ///
    /// `index` — zero-based track index.
    #[napi]
    pub fn jump_to(&self, index: i32) {
        self.inner.send(PlayerCommand::JumpTo(index as usize));
    }

    /// Randomly reorder all tracks in the playlist.
    #[napi]
    pub fn shuffle_playlist(&self) {
        self.inner.send(PlayerCommand::AleatoryFullRandom);
    }

    // -- Getters --

    /// Whether the player is currently playing.
    #[napi]
    pub fn is_playing(&self) -> bool {
        self.inner.is_playing()
    }

    /// Current playback position in seconds.
    #[napi]
    pub fn position(&self) -> f64 {
        self.inner.position() as f64
    }

    /// Duration of the current track in seconds.
    #[napi]
    pub fn duration(&self) -> f64 {
        self.inner.duration() as f64
    }

    /// Current volume level in the range `[0.0, 1.0]`.
    #[napi]
    pub fn volume(&self) -> f64 {
        self.inner.volume() as f64
    }

    /// Whether shuffle mode is enabled.
    #[napi]
    pub fn shuffle(&self) -> bool {
        self.inner.shuffle()
    }

    /// Whether repeat-all mode is enabled.
    #[napi]
    pub fn repeat(&self) -> bool {
        self.inner.repeat()
    }

    /// Whether repeat-one mode is enabled.
    #[napi]
    pub fn repeat_one(&self) -> bool {
        self.inner.repeat_one()
    }

    /// Current loudness level per channel.
    ///
    /// Returns a two-element vector `[left, right]` with values in `[0.0, 1.0]`.
    #[napi]
    pub fn get_loudness(&self) -> Vec<f64> {
        let (l, r) = self.inner.get_loudness();
        vec![l as f64, r as f64]
    }

    /// Sample rate of the current audio output in Hz.
    #[napi]
    pub fn get_sample_rate(&self) -> f64 {
        self.inner.get_sample_rate() as f64
    }

    /// Number of tracks in the current playlist.
    #[napi]
    pub fn playlist_length(&self) -> i32 {
        self.inner.playlist().len() as i32
    }

    /// Index of the currently playing track, or `-1` if no track is loaded.
    #[napi]
    pub fn playlist_index(&self) -> i32 {
        self.inner.playlist_idx() as i32
    }

    /// Raw cover art bytes (JPEG/PNG) of the current track, or an empty buffer.
    #[napi]
    pub fn cover(&self) -> Vec<u8> {
        self.inner.cover().data
    }

    /// Metadata for the current track, or `None` if no track is loaded.
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

    /// Register a callback for player events.
    ///
    /// Spawns a background thread that polls the player and calls
    /// `callback` with a JSON-encoded event string for each event.
    /// Only one callback can be active at a time; calling this again
    /// will spawn a new thread (the old one is orphaned).
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

    /// Poll for the next pending event.
    ///
    /// Returns a JSON string or `None` if no event is available.
    /// Use this instead of [`set_event_callback`](Self::set_event_callback)
    /// if you prefer manual polling.
    #[napi]
    pub fn poll_event(&self) -> Option<String> {
        self.inner.try_recv_event().as_ref().map(event_to_json)
    }

    // -- Cleanup --

    /// Signal the event callback thread to shut down.
    ///
    /// Safe to call multiple times. The player is also cleaned up
    /// automatically when the `JsPlayer` instance is garbage-collected.
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
