//! Discovers installed apps and launches them.
//!
//! On Windows the list comes from the shell's Applications folder, which is what the built-in
//! Start menu shows. On other platforms a fixed sample list stands in so the UI can be iterated on.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::{launch, load};

#[cfg(not(windows))]
mod preview;
#[cfg(not(windows))]
pub use preview::{launch, load};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// Name shown in the menu.
    pub name: String,
    /// Platform-specific identifier passed back to `launch`.
    pub launch_id: String,
}
