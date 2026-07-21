//! Event-callback registration for C callers.
//!
//! When a callback is registered the engine spawns a dedicated Rust thread
//! that polls for events every ~16 ms and forwards them to the C function.

use super::handle::OpaquePlayer;
use super::types::CEvent;

/// Callback invoked by the engine when a new event is available.
///
/// - `CEvent` — the event data (see [`CEvent`] for kind semantics).
/// - `*mut c_void` — opaque user-data pointer passed at registration time.
pub type EventCallback = extern "C" fn(CEvent, *mut std::ffi::c_void);

/// Registers an event callback that runs on a dedicated polling thread.
///
/// Once registered the callback is invoked for every engine event until a
/// [`Shutdown`](crate::engine::event::Event::Shutdown) event is received,
/// at which point the polling thread exits.
///
/// Returns `0` on success or `-1` when `ptr` is null.
///
/// # Safety
///
/// - `ptr` must be a valid, non-null pointer from [`pc_player_create`](super::api::pc_player_create).
/// - `cb` must be a valid function pointer.
/// - `user_data` is passed through to every callback invocation; the caller
///   is responsible for its lifetime.
/// - The callback is called from a **separate Rust thread**, not from the
///   audio callback.  It is safe to call back into the player API from the
///   callback.
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
