//! Linux trigger: the host's D-Bus interface on the session bus.
//!
//! Compositor bindings call `Toggle()` (KWin's `[ModifierOnlyShortcuts] Meta` entry), and a second
//! `rusty-start` launch calls `ToggleWithToken(token)`. Method calls arrive on zbus's executor
//! thread, so `on_toggle` must hand the work to the UI thread.

use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result};
use rusty_start_protocol::DBUS_OBJECT_PATH;
use zbus::blocking::Connection;

use crate::dbus;

/// The compositor sees the Meta key even while the menu has keyboard focus and calls `Toggle()`,
/// so a tap handled in the window as well would close the menu and reopen it at once.
pub const MENU_HANDLES_WIN_KEY: bool = false;

/// The compositor owns the Meta key, so there is nothing to suppress.
pub fn suppress_native_start_menu() {}

/// Serves the toggle interface until dropped.
pub struct TriggerGuard {
    connection: Connection,
}

impl Drop for TriggerGuard {
    fn drop(&mut self) {
        let _ = self.connection.object_server().remove::<ToggleService, _>(DBUS_OBJECT_PATH);
    }
}

/// Serves the toggle interface. `on_toggle` runs on zbus's executor thread, not on the UI thread.
pub fn install(on_toggle: impl Fn() + Send + 'static) -> Result<TriggerGuard> {
    let connection: Connection = dbus::session()?;
    let service = ToggleService { on_toggle: Mutex::new(Box::new(on_toggle)) };
    connection.object_server().at(DBUS_OBJECT_PATH, service).context("failed to serve the toggle interface")?;
    Ok(TriggerGuard { connection })
}

struct ToggleService {
    on_toggle: Mutex<Box<dyn Fn() + Send>>,
}

impl ToggleService {
    fn run_toggle(&self) {
        (self.on_toggle.lock().unwrap_or_else(PoisonError::into_inner))();
    }
}

#[zbus::interface(name = "io.github.jav76.RustyStart")]
impl ToggleService {
    fn toggle(&self) {
        self.run_toggle();
    }

    /// Slint rebuilds the window after every hide without a way to pass an activation token, so
    /// the token is not applied yet and focus follows the compositor's policy (see the KWin
    /// window rule that `rusty-start setup` installs).
    fn toggle_with_token(&self, token: &str) {
        let _ = token;
        self.run_toggle();
    }
}

#[cfg(test)]
mod tests {
    use rusty_start_protocol::{DBUS_INTERFACE, DBUS_TOGGLE_METHOD, DBUS_TOGGLE_WITH_TOKEN_METHOD};
    use zbus::object_server::Interface;

    use super::*;

    #[test]
    fn interface_name_matches_the_protocol() {
        assert_eq!(<ToggleService as Interface>::name().as_str(), DBUS_INTERFACE);
    }

    #[test]
    fn method_names_match_the_protocol() {
        let mut xml = String::new();
        let service = ToggleService { on_toggle: Mutex::new(Box::new(|| {})) };
        service.introspect_to_writer(&mut xml, 0);

        assert!(xml.contains(&format!("<method name=\"{DBUS_TOGGLE_METHOD}\"")));
        assert!(xml.contains(&format!("<method name=\"{DBUS_TOGGLE_WITH_TOKEN_METHOD}\"")));
    }
}
