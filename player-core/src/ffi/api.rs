//! `extern "C"` functions exposed by the `c-ffi` feature.
//!
//! All functions accept or return an opaque `*mut OpaquePlayer` handle.
//! Null-pointer checks are performed at the top of every entry point.

use super::handle::OpaquePlayer;
use super::types::{CCommand, CEvent, CPlayerState};
use crate::api::builder::PlayerBuilder;
use crate::engine::command::PlayerCommand;

/// Creates a new player instance.
///
/// Returns an opaque pointer that must be destroyed with
/// [`pc_player_destroy`].  The player is initialised in a stopped state.
///
/// # Safety
///
/// The caller must eventually call [`pc_player_destroy`] on the returned
/// pointer to avoid a memory leak.
#[no_mangle]
pub extern "C" fn pc_player_create(volume: f32) -> *mut OpaquePlayer {
    let player = PlayerBuilder::new()
        .with_volume(volume)
        .build();

    Box::into_raw(Box::new(OpaquePlayer {
        inner: player,
        error_buf: std::sync::Mutex::new(String::new()),
    }))
}

/// Destroys a player instance created by [`pc_player_create`].
///
/// # Safety
///
/// `ptr` must have been returned by [`pc_player_create`] and must not have
/// been destroyed already (no double-free).  A null pointer is safely
/// handled as a no-op.
#[no_mangle]
pub extern "C" fn pc_player_destroy(ptr: *mut OpaquePlayer) {
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)); }
    }
}

/// Sends a [`CCommand`] to the player engine.
///
/// Returns `0` on success or `-1` when `ptr` is null.
///
/// # Safety
///
/// `ptr` must be a valid, non-null pointer returned by [`pc_player_create`]
/// that has not yet been destroyed.
#[no_mangle]
pub extern "C" fn pc_player_send(ptr: *mut OpaquePlayer, cmd: CCommand) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let player = unsafe { &(*ptr).inner };
    let command = match cmd {
        CCommand::Play => PlayerCommand::Play,
        CCommand::Pause => PlayerCommand::Pause,
        CCommand::Stop => PlayerCommand::Stop,
        CCommand::Next => PlayerCommand::Next,
        CCommand::Prev => PlayerCommand::Prev,
        CCommand::ToggleShuffle => PlayerCommand::ToggleShuffle,
        CCommand::ToggleRepeat => PlayerCommand::ToggleRepeat,
        CCommand::ToggleRepeatOne => PlayerCommand::ToggleRepeatOne,
        CCommand::SetVolume(v) => PlayerCommand::SetVolume(v),
        CCommand::Seek(t) => PlayerCommand::Seek(t),
    };
    player.send(command);
    0
}

/// Returns `true` when the player is actively playing audio.
///
/// # Safety
///
/// `ptr` must be valid and non-null.  Returns `false` when `ptr` is null.
#[no_mangle]
pub extern "C" fn pc_player_is_playing(ptr: *mut OpaquePlayer) -> bool {
    if ptr.is_null() {
        return false;
    }
    let player = unsafe { &(*ptr).inner };
    player.is_playing()
}

/// Returns the current playback position in seconds.
///
/// # Safety
///
/// `ptr` must be valid and non-null.  Returns `0.0` when `ptr` is null.
#[no_mangle]
pub extern "C" fn pc_player_position(ptr: *mut OpaquePlayer) -> f32 {
    if ptr.is_null() {
        return 0.0;
    }
    let player = unsafe { &(*ptr).inner };
    player.position()
}

/// Writes a snapshot of the current player state into `out`.
///
/// Returns `0` on success or `-1` when either pointer is null.
///
/// # Safety
///
/// Both `ptr` and `out` must be valid, non-null pointers to suitably sized
/// and aligned memory.  `out` is written (not appended to).
#[no_mangle]
pub extern "C" fn pc_player_get_state(ptr: *mut OpaquePlayer, out: *mut CPlayerState) -> i32 {
    if ptr.is_null() || out.is_null() {
        return -1;
    }
    let player = unsafe { &(*ptr).inner };
    let state = player.state.lock().unwrap();
    unsafe {
        *out = CPlayerState {
            playing: state.playing,
            volume: state.volume,
            position: state.position,
            duration: state.duration,
            shuffle: state.shuffle,
            repeat_one: state.repeat_one,
            repeat: state.repeat,
            playlist_len: state.playlist.len(),
            playlist_idx: state.playlist_idx,
            sample_rate: state.sample_rate,
        };
    }
    0
}

/// Polls for a pending engine event without blocking.
///
/// Writes the event into `out` and returns `1` when an event was available,
/// or returns `0` when no event is pending.  Returns `-1` when either
/// pointer is null.
///
/// # Safety
///
/// Both `ptr` and `out` must be valid, non-null pointers to suitably sized
/// and aligned memory.
#[no_mangle]
pub extern "C" fn pc_player_poll_event(ptr: *mut OpaquePlayer, out: *mut CEvent) -> i32 {
    if ptr.is_null() || out.is_null() {
        return -1;
    }
    let player = unsafe { &(*ptr).inner };
    match player.try_recv_event() {
        Some(event) => {
            let (kind, int_arg, float_arg) = match event {
                crate::engine::event::Event::StateChanged => (0, 0, 0.0),
                crate::engine::event::Event::TrackChanged(idx) => (1, idx as i64, 0.0),
                crate::engine::event::Event::PlaylistChanged => (2, 0, 0.0),
                crate::engine::event::Event::Loudness(l, r) => (3, 0, l as f64),
                crate::engine::event::Event::Error(_) => (4, 0, 0.0),
                crate::engine::event::Event::Shutdown => (5, 0, 0.0),
            };
            unsafe {
                *out = CEvent { kind, int_arg, float_arg };
            }
            1
        }
        None => 0,
    }
}
