//! Keeps one host running per session.
//!
//! On Windows a named mutex marks the running host; on Linux the host owns its well-known name on
//! the session bus. A second launch finds the host taken and sends it a request (toggle its menu,
//! or quit) instead of installing a second set of triggers. In preview builds every launch runs and
//! there is no host to send to.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{InstanceGuard, acquire, send};

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::{InstanceGuard, acquire, send};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{InstanceGuard, acquire, send};

/// What a launch asks the host that already runs in this session to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRequest {
    Toggle,
    Quit,
}
