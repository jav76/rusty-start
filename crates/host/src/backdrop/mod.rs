//! Captures the active monitor's desktop pixels and geometry before the Start menu window opens.
//!
//! On Windows, captures the monitor containing the mouse cursor (`rcMonitor`, including the taskbar)
//! via a reusable GDI DIB section so the GPU shader can fog the screen and refract desktop pixels
//! through the Liquid-Glass Card. On Linux the overlay is a fullscreen Wayland window and KWin
//! captures the screen. Preview builds generate a synthetic desktop scene so the shader pipeline
//! can be exercised.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{
    Capturer, OVERLAY_FOLLOWS_WINDOW_SIZE, OverlayOwner, active_monitor_rect, configure_overlay_window,
    focus_overlay_window, install_backend, keep_overlay_fullscreen, place_overlay_window, prepare_overlay_surface,
};

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
mod plasma_shell;
#[cfg(backend_linux)]
pub use linux::{
    Capturer, OVERLAY_FOLLOWS_WINDOW_SIZE, OverlayOwner, active_monitor_rect, configure_overlay_window,
    focus_overlay_window, install_backend, keep_overlay_fullscreen, place_overlay_window, prepare_overlay_surface,
};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{
    Capturer, OVERLAY_FOLLOWS_WINDOW_SIZE, OverlayOwner, active_monitor_rect, configure_overlay_window,
    focus_overlay_window, install_backend, keep_overlay_fullscreen, place_overlay_window, prepare_overlay_surface,
};

#[cfg(backend_preview)]
mod synthetic;

#[cfg(any(backend_linux, test))]
mod kwin_image;

/// Capture error meaning the backend cannot capture for the rest of the session. The backend
/// already logged why, so callers skip logging it again.
#[derive(Debug)]
pub struct CaptureUnavailable;

impl std::fmt::Display for CaptureUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("desktop capture is unavailable this session")
    }
}

impl std::error::Error for CaptureUnavailable {}

/// Virtual-screen rectangle of a monitor in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Captured desktop image for a single monitor, stored in bottom-up RGBA order for OpenGL upload.
#[derive(Clone, Debug)]
pub struct MonitorSnapshot {
    pub rect: MonitorRect,
    pub rgba_bottom_up: Vec<u8>,
}
