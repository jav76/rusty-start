//! Wayland overlay window for KDE Plasma (KWin), and other compositors that honor fullscreen
//! requests.
//!
//! Wayland clients cannot position their windows, so the overlay asks for fullscreen on the output
//! the compositor picks, which is the active one on KWin. Slint destroys the window on every hide
//! under Wayland and rebuilds its winit attributes without the backend's attributes hook, so
//! fullscreen is requested again before every show through Slint's own window properties.
//!
//! The backdrop is still the synthetic scene until KWin ScreenShot2 capture lands.

use anyhow::{Context, Result};
use rusty_start_protocol::APP_ID;
use slint::winit_030::winit::platform::startup_notify::WindowAttributesExtStartupNotify;
use slint::winit_030::winit::window::ActivationToken;

use super::MonitorRect;
pub use super::synthetic::Capturer;
use super::synthetic::SYNTHETIC_MONITOR_RECT;

/// The compositor sizes the fullscreen overlay, so the layout follows the window's size.
pub const OVERLAY_FOLLOWS_WINDOW_SIZE: bool = true;

/// Size used for layout until the compositor reports the fullscreen size.
#[expect(clippy::unnecessary_wraps, reason = "signature matches the fallible win32 backend")]
pub fn active_monitor_rect() -> Result<MonitorRect> {
    Ok(SYNTHETIC_MONITOR_RECT)
}

/// Placeholder so the host's startup code is identical on every platform.
pub struct OverlayOwner;

/// Selects the winit backend and sets the app ID that KWin rules and the `.desktop` file match.
/// Must run on the UI thread before the first Slint component is created.
pub fn install_backend() -> Result<OverlayOwner> {
    // A launcher that started the host (a shortcut, the session's autostart) hands over an
    // activation token, which lets the first window take keyboard focus. Later windows rely on
    // the compositor's focus policy, because Slint rebuilds their attributes without this hook.
    let startup_token: Option<String> = std::env::var("XDG_ACTIVATION_TOKEN").ok().filter(|token| !token.is_empty());

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(move |attributes| match &startup_token {
            Some(token) => attributes.with_activation_token(ActivationToken::from_raw(token.clone())),
            None => attributes,
        })
        .select()
        .context("failed to select the winit backend")?;
    slint::set_xdg_app_id(APP_ID).context("failed to set the Wayland app ID")?;

    Ok(OverlayOwner)
}

/// Requests fullscreen for the next window Slint creates. Turning it off first makes Slint copy
/// the state into the attributes it rebuilt after the previous hide. The compositor picks the size,
/// so `rect` is ignored: setting a size would shrink the first window back out of fullscreen.
pub fn place_overlay_window(window: &slint::Window, _rect: MonitorRect) {
    window.set_fullscreen(false);
    window.set_fullscreen(true);
}

pub fn configure_overlay_window() {}

/// The compositor decides focus on Wayland. The menu learns the outcome from focus events.
pub fn focus_overlay_window() -> bool {
    true
}
