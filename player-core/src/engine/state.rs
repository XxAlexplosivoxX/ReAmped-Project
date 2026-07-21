//! Shared playback state visible to the rest of the application.
//!
//! [`PlayerState`] is behind `Arc<Mutex<PlayerState>>` so the audio thread and
//! any other thread (UI, CLI, …) can read or write fields concurrently.

use crate::metadata::{CoverArt, Metadata};
use super::track::Track;

/// A snapshot of the current playback state.
///
/// All fields are `pub` so that both the audio loop and external consumers
/// can inspect or modify them under the mutex guard.
#[derive(Clone, Debug)]
pub struct PlayerState {
    /// Whether a track is currently playing (true) or paused/stopped (false).
    pub playing: bool,
    /// Output volume in the range `0.0` … `1.0`.
    pub volume: f32,
    /// Current playback position in seconds.
    pub position: f32,
    /// Total duration of the current track in seconds (after silence trim).
    pub duration: f32,
    /// Display title of the current track.
    pub current_track: String,
    /// Cover art bitmap for the current track.
    pub cover: CoverArt,
    /// Parsed metadata from the current track's file tags.
    pub metadata: Option<Metadata>,
    /// Whether shuffle mode is active.
    pub shuffle: bool,
    /// Whether repeat-one mode is active.
    pub repeat_one: bool,
    /// Whether repeat-all mode is active.
    pub repeat: bool,
    /// The current working playlist (may be reordered by sort/shuffle).
    pub playlist: Vec<Track>,
    /// An immutable copy of the original playlist order (restored by `SortBy::Normal`).
    pub playlist_cpy: Vec<Track>,
    /// Index into `playlist` of the currently playing track.
    pub playlist_idx: usize,
    /// Sample rate of the audio backend in Hz.
    pub sample_rate: f32,
}
