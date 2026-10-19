//! Decides when the menu opens.
//!
//! On Windows a message-only window listens for a development hotkey and for the toggle message
//! the explorer hook will post. On other platforms there is no global trigger, so the menu opens
//! once at startup to allow UI iteration.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::{TriggerGuard, install};

#[cfg(not(windows))]
mod preview;
#[cfg(not(windows))]
pub use preview::{TriggerGuard, install};
