//! Opaque handle type for FFI consumers.
//!
//! [`OpaquePlayer`] wraps the Rust [`Player`] behind a raw pointer, hiding
//! all internal layout from C callers.

use std::sync::Mutex;

use crate::api::handle::Player;

/// Opaque wrapper around [`Player`] for the C FFI.
///
/// Instances are heap-allocated via [`Box::into_raw`] by
/// [`pc_player_create`](super::api::pc_player_create) and must be freed
/// through [`pc_player_destroy`](super::api::pc_player_destroy).  The
/// `error_buf` captures human-readable error messages for the C caller.
pub struct OpaquePlayer {
    pub inner: Player,
    pub error_buf: Mutex<String>,
}
