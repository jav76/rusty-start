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

/// Program name of a launch ID that is a path to an `.exe`, such as `msedge` for
/// `{6D809377-...}\Microsoft\Edge\Application\msedge.exe`, so apps are found by the name
/// typed in Run dialogs. Packaged apps (AppUserModelIDs) have none.
#[cfg(any(backend_win32, test))]
pub fn exe_keyword(launch_id: &str) -> Option<String> {
    let file_name: &str = launch_id.rsplit(['\\', '/']).next()?;
    let (stem, extension) = file_name.rsplit_once('.')?;

    (extension.eq_ignore_ascii_case("exe") && !stem.is_empty()).then(|| stem.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_keyword_is_the_program_file_name_without_extension() {
        assert_eq!(
            exe_keyword("{6D809377-6AF0-444B-8957-A3773F02200E}\\Tools\\mspaint.EXE").as_deref(),
            Some("mspaint")
        );
        assert_eq!(exe_keyword("C:/Apps/my.tool.exe").as_deref(), Some("my.tool"));
    }

    #[test]
    fn packaged_apps_and_other_files_have_no_exe_keyword() {
        assert_eq!(exe_keyword("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"), None);
        assert_eq!(exe_keyword("Microsoft.Office.WINWORD.EXE.15"), None);
        assert_eq!(exe_keyword("{GUID}\\Docs\\readme.txt"), None);
        assert_eq!(exe_keyword("{GUID}\\.exe"), None);
    }
}
