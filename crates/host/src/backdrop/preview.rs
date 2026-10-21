//! Preview backend: a fixed-size window over the synthetic desktop backdrop.

#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible win32 backend")]

use anyhow::Result;
use slint::{PhysicalPosition, PhysicalSize};

use super::MonitorRect;
pub use super::synthetic::Capturer;
use super::synthetic::SYNTHETIC_MONITOR_RECT;

/// The preview window keeps the size it is given.
pub const OVERLAY_FOLLOWS_WINDOW_SIZE: bool = false;

pub fn active_monitor_rect() -> Result<MonitorRect> {
    Ok(SYNTHETIC_MONITOR_RECT)
}

/// Placeholder so the host's startup code is identical on every platform.
pub struct OverlayOwner;

pub fn install_backend() -> Result<OverlayOwner> {
    Ok(OverlayOwner)
}

pub fn place_overlay_window(window: &slint::Window, rect: MonitorRect) {
    window.set_position(PhysicalPosition::new(rect.x, rect.y));
    window.set_size(PhysicalSize::new(rect.width, rect.height));
}

/// The overlay is not fullscreen on this backend, so it never needs restoring.
pub fn keep_overlay_fullscreen(_window: &slint::Window) -> bool {
    true
}

pub fn configure_overlay_window() {}

pub fn prepare_overlay_surface(_window: &slint::Window) {}

pub fn focus_overlay_window() -> bool {
    true
}
