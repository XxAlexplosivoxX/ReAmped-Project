use super::handle::OpaquePlayer;
use super::types::CEvent;

pub type EventCallback = extern "C" fn(CEvent, *mut std::ffi::c_void);

#[no_mangle]
pub extern "C" fn pc_player_register_callback(
    ptr: *mut OpaquePlayer,
    cb: EventCallback,
    user_data: *mut std::ffi::c_void,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let player = unsafe { &(*ptr).inner };
    let cb = cb.clone();
    let player_clone = player.clone();
    std::thread::spawn(move || {
        loop {
            if let Some(event) = player_clone.try_recv_event() {
                let (kind, int_arg, float_arg) = match event {
                    crate::engine::event::Event::StateChanged => (0, 0, 0.0),
                    crate::engine::event::Event::TrackChanged(idx) => (1, idx as i64, 0.0),
                    crate::engine::event::Event::PlaylistChanged => (2, 0, 0.0),
                    crate::engine::event::Event::Loudness(l, r) => (3, 0, l as f64),
                    crate::engine::event::Event::Error(_) => (4, 0, 0.0),
                    crate::engine::event::Event::Shutdown => break,
                };
                let c_event = CEvent { kind, int_arg, float_arg };
                cb(c_event, user_data);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    });
    0
}
