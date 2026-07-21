//! C-compatible FFI layer for embedding the player in non-Rust hosts.
//!
//! # Safety model
//!
//! - Every public `extern "C"` function performs a **null-pointer guard** on
//!   its [`OpaquePlayer`](handle::OpaquePlayer) argument before dereferencing.
//! - Output pointers (e.g. `out` parameters) are also checked for null.
//! - Mutexes that guard [`PlayerState`](crate::engine::state::PlayerState) are
//!   locked and released *within* each FFI call — they are never held across
//!   the boundary.
//! - The opaque handle pattern (`*mut OpaquePlayer`) prevents C code from
//!   inspecting or mutating Rust internals directly.
//!
//! Feature gate: `c-ffi`.

pub mod handle;
pub mod types;
pub mod api;
pub mod callbacks;
