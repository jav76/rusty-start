//! Keeps one host running per session.
//!
//! On Windows a named mutex marks the running host. A second launch toggles that host's menu and
//! exits instead of installing a second set of hooks. On other platforms every launch runs.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{InstanceGuard, acquire};

#[cfg(not(backend_win32))]
mod preview;
#[cfg(not(backend_win32))]
pub use preview::{InstanceGuard, acquire};
