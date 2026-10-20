//! Captures the active monitor's desktop pixels and geometry before the Start menu window opens.
//!
//! On Windows, captures the monitor containing the mouse cursor (`rcMonitor`, including the taskbar)
//! via a reusable GDI DIB section so the GPU shader can fog the screen and refract desktop pixels
//! through the Liquid-Glass Card. On other backends, generates a synthetic desktop scene so the
//! shader pipeline can be exercised in preview builds.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{
    Capturer, OverlayOwner, active_monitor_rect, configure_overlay_window, focus_overlay_window, install_backend,
    place_overlay_window,
};

#[cfg(not(backend_win32))]
mod preview;
#[cfg(not(backend_win32))]
pub use preview::{
    Capturer, OverlayOwner, active_monitor_rect, configure_overlay_window, focus_overlay_window, install_backend,
    place_overlay_window,
};

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
