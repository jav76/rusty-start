//! Captures the active monitor's desktop pixels and geometry before the Start menu window opens.
//!
//! On Windows, captures the monitor containing the mouse cursor (`rcMonitor`, including the taskbar)
//! via a reusable GDI DIB section so the GPU shader can fog the screen and refract desktop pixels
//! through the Liquid-Glass Card. On non-Windows platforms, generates a synthetic desktop scene so
//! the shader pipeline can be exercised in preview builds.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::Capturer;

#[cfg(not(windows))]
mod preview;
#[cfg(not(windows))]
pub use preview::Capturer;

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

