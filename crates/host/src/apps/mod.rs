//! Discovers installed apps, renders their icons and launches them.
//!
//! On Windows the list comes from the shell's Applications folder, which is what the built-in
//! Start menu shows. On Linux it comes from the freedesktop Desktop Entry files that desktop
//! launchers list. Preview builds use a fixed sample list so the UI can be iterated on.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{AppWatcher, actions, launch, load, load_icons, perform_action, watch};

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::{AppWatcher, actions, launch, load, load_icons, perform_action, watch};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{AppWatcher, actions, launch, load, load_icons, perform_action, watch};

#[cfg(any(backend_linux, test))]
mod desktop_entry;
#[cfg(any(backend_linux, test))]
mod exec;
#[cfg(any(backend_linux, test))]
mod icon_theme;
#[cfg(any(backend_linux, test))]
mod raster;
#[cfg(any(backend_win32, test))]
mod shell_id;

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
    /// Other words the app is found by: its generic name and keywords on Linux, its program file
    /// name on Windows.
    pub keywords: Vec<String>,
}

impl Searchable for App {
    fn name(&self) -> &str {
        &self.name
    }

    fn keywords(&self) -> &[String] {
        &self.keywords
    }
}

/// Something the App Action Flyout offers for an app besides pinning. Each backend lists the
/// actions an app supports in flyout order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// One of the desktop entry's own actions, such as "New Private Window", by its action ID.
    #[cfg_attr(backend_win32, expect(dead_code, reason = "only desktop entries on Linux have their own actions"))]
    Desktop {
        id: String,
        name: String,
    },
    RunAsAdministrator,
    OpenFileLocation,
    #[cfg_attr(
        all(backend_linux, not(test)),
        expect(dead_code, reason = "Linux has no uninstaller every desktop shares, so it never offers this")
    )]
    Uninstall,
}

impl AppAction {
    pub fn label(&self) -> &str {
        match self {
            Self::Desktop { name, .. } => name,
            Self::RunAsAdministrator => "Run as administrator",
            Self::OpenFileLocation => "Open file location",
            Self::Uninstall => "Uninstall",
        }
    }

    /// Whether the action starts the app, so it counts as a launch in the history.
    pub fn launches_app(&self) -> bool {
        matches!(self, Self::Desktop { .. } | Self::RunAsAdministrator)
    }
}
