//! Discovers installed apps, renders their icons and launches them.
//!
//! On Windows the list comes from the shell's Applications folder, which is what the built-in
//! Start menu shows. On Linux it comes from the freedesktop Desktop Entry files that desktop
//! launchers list. Preview builds use a fixed sample list so the UI can be iterated on.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{AppWatcher, launch, load, load_icons, watch};

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::{AppWatcher, launch, load, load_icons, watch};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{AppWatcher, launch, load, load_icons, watch};

#[cfg(any(backend_linux, test))]
mod desktop_entry;
#[cfg(any(backend_linux, test))]
mod exec;
#[cfg(any(backend_linux, test))]
mod icon_theme;
#[cfg(any(backend_linux, test))]
mod raster;

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
