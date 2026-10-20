//! Keeps one host running per session.
//!
//! On Windows a named mutex marks the running host; on Linux the host owns its well-known name on
//! the session bus. A second launch toggles that host's menu and exits instead of installing a
//! second set of triggers. In preview builds every launch runs.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{InstanceGuard, acquire};

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::{InstanceGuard, acquire};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{InstanceGuard, acquire};
