//! Keeps one host running per session.
//!
//! On Windows a named mutex marks the running host. A second launch toggles that host's menu and
//! exits instead of installing a second set of hooks. On other platforms every launch runs.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::{InstanceGuard, acquire};

#[cfg(not(windows))]
mod preview;
#[cfg(not(windows))]
pub use preview::{InstanceGuard, acquire};
