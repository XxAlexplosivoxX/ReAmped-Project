use super::handle::OpaquePlayer;
use super::types::{CCommand, CEvent, CPlayerState};
use crate::api::builder::PlayerBuilder;
use crate::engine::command::PlayerCommand;

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

#[no_mangle]
pub extern "C" fn pc_player_destroy(ptr: *mut OpaquePlayer) {
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)); }
    }
}

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

#[no_mangle]
pub extern "C" fn pc_player_is_playing(ptr: *mut OpaquePlayer) -> bool {
    if ptr.is_null() {
        return false;
    }
    let player = unsafe { &(*ptr).inner };
    player.is_playing()
}

#[no_mangle]
pub extern "C" fn pc_player_position(ptr: *mut OpaquePlayer) -> f32 {
    if ptr.is_null() {
        return 0.0;
    }
    let player = unsafe { &(*ptr).inner };
    player.position()
}

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
