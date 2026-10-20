//! Discovers installed apps, renders their icons and launches them.
//!
//! On Windows the list comes from the shell's Applications folder, which is what the built-in
//! Start menu shows. On other platforms a fixed sample list stands in so the UI can be iterated on.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::{AppWatcher, launch, load, load_icons, watch};

#[cfg(not(windows))]
mod preview;
#[cfg(not(windows))]
pub use preview::{AppWatcher, launch, load, load_icons, watch};

use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::search::Searchable;

/// Icon edge in physical pixels, sharp at the 24px logical icon size up to 200% scaling.
pub const ICON_SIZE: u32 = 48;

/// Premultiplied RGBA icon. Built off the UI thread, then wrapped in a `slint::Image` on it.
pub type IconPixels = SharedPixelBuffer<Rgba8Pixel>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// Name shown in the menu.
    pub name: String,
    /// Platform-specific identifier passed back to `launch`. Pins and launch history use it too.
    pub launch_id: String,
}

impl Searchable for App {
    fn name(&self) -> &str {
        &self.name
    }
}
