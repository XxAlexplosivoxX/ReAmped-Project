use std::sync::Mutex;

use crate::api::handle::Player;

pub struct OpaquePlayer {
    pub inner: Player,
    pub error_buf: Mutex<String>,
}
