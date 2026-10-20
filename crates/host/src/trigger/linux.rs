//! Linux trigger: the host's D-Bus interface on the session bus, and its KDE global shortcut.
//!
//! Desktop bindings call `Toggle()`, and a second `rusty-start` launch calls
//! `ToggleWithToken(token)`. On KDE Plasma the host also registers a kglobalaccel action, which
//! `rusty-start setup` binds to the Meta key, and listens for its presses on the
//! `rusty-start-shortcut` thread. Toggles arrive on those threads, so `on_toggle` must hand the work
//! to the UI thread.
//!
//! The interface is served before the host claims its bus name (see `serve_toggle_interface`), so
//! the call that started the host through D-Bus activation is queued instead of lost.

use std::sync::{Mutex, PoisonError};

use std::thread;

use anyhow::{Context, Result, bail};
use rusty_start_protocol::{DBUS_OBJECT_PATH, GLOBAL_SHORTCUT_ACTION_ID};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

use crate::dbus;

/// The desktop sees the Meta key even while the menu has keyboard focus and toggles the menu, so a
/// tap handled in the window as well would close the menu and reopen it at once.
pub const MENU_HANDLES_WIN_KEY: bool = false;

static HANDLER: Mutex<Handler> = Mutex::new(Handler { on_toggle: None, queued: false });

struct Handler {
    on_toggle: Option<Box<dyn Fn() + Send>>,
    /// A toggle arrived before `install`, typically the call that activated the host.
    queued: bool,
}

/// The compositor owns the Meta key, so there is nothing to suppress.
pub fn suppress_native_start_menu() {}

/// Stops handling toggles when dropped.
pub struct TriggerGuard {
    connection: Connection,
}

impl Drop for TriggerGuard {
    fn drop(&mut self) {
        let _ = self.connection.object_server().remove::<ToggleService, _>(DBUS_OBJECT_PATH);
        HANDLER.lock().unwrap_or_else(PoisonError::into_inner).on_toggle = None;
    }
}

/// Serves the toggle interface on `connection`. Toggles queue until `install` provides a handler.
/// Must run before the host claims its bus name.
pub fn serve_toggle_interface(connection: &Connection) -> Result<()> {
    connection.object_server().at(DBUS_OBJECT_PATH, ToggleService).context("failed to serve the toggle interface")?;
    Ok(())
}

/// Routes toggles to `on_toggle`, running one first if a toggle was queued. `on_toggle` runs on
/// zbus's executor thread, not on the UI thread.
pub fn install(on_toggle: impl Fn() + Send + 'static) -> Result<TriggerGuard> {
    let connection: Connection = dbus::session()?;
    let mut handler = HANDLER.lock().unwrap_or_else(PoisonError::into_inner);
    if std::mem::take(&mut handler.queued) {
        on_toggle();
    }
    handler.on_toggle = Some(Box::new(on_toggle));
    drop(handler);

    let shortcut_connection: Connection = connection.clone();
    let spawned = thread::Builder::new().name("rusty-start-shortcut".into()).spawn(move || {
        if let Err(error) = listen_for_global_shortcut(&shortcut_connection) {
            eprintln!("the KDE global shortcut is unavailable: {error:#}");
        }
    });
    if let Err(error) = spawned {
        eprintln!("failed to spawn the global shortcut thread: {error}");
    }
    Ok(TriggerGuard { connection })
}

/// Marks the kglobalaccel action present, so its saved keys (Meta, after `rusty-start setup`)
/// trigger it while the host runs, then toggles on every press. Returns when kglobalaccel is
/// missing, which is normal outside KDE Plasma.
fn listen_for_global_shortcut(connection: &Connection) -> Result<()> {
    const SET_PRESENT: u32 = 2;
    let kglobalaccel = Proxy::new(connection, "org.kde.kglobalaccel", "/kglobalaccel", "org.kde.KGlobalAccel")
        .context("no kglobalaccel proxy")?;
    let action_id: Vec<&str> = GLOBAL_SHORTCUT_ACTION_ID.to_vec();
    if kglobalaccel.call_method("doRegister", &(action_id.clone(),)).is_err() {
        // Not KDE Plasma: there is no kglobalaccel to register with.
        return Ok(());
    }
    // `a(ai)`: one struct per key sequence, each holding up to four Qt key codes.
    let no_keys: Vec<(Vec<i32>,)> = Vec::new();
    kglobalaccel
        .call_method("setShortcutKeys", &(action_id, no_keys, SET_PRESENT))
        .context("failed to mark the shortcut present")?;
    let component: OwnedObjectPath = kglobalaccel
        .call("getComponent", &(GLOBAL_SHORTCUT_ACTION_ID[0],))
        .context("failed to find the shortcut's kglobalaccel component")?;

    let component_proxy = Proxy::new(connection, "org.kde.kglobalaccel", component, "org.kde.kglobalaccel.Component")
        .context("no kglobalaccel component proxy")?;
    let presses =
        component_proxy.receive_signal("globalShortcutPressed").context("failed to watch shortcut presses")?;
    for press in presses {
        let (_, action, _): (String, String, i64) =
            press.body().deserialize().context("unexpected globalShortcutPressed signal")?;
        if action == GLOBAL_SHORTCUT_ACTION_ID[1] {
            run_toggle();
        }
    }
    bail!("kglobalaccel stopped sending shortcut presses")
}

fn run_toggle() {
    let mut handler = HANDLER.lock().unwrap_or_else(PoisonError::into_inner);
    match &handler.on_toggle {
        Some(on_toggle) => on_toggle(),
        None => handler.queued = true,
    }
}

struct ToggleService;

#[zbus::interface(name = "io.github.jav76.RustyStart")]
impl ToggleService {
    #[expect(clippy::unused_self, reason = "zbus interface methods take &self; the handler lives in HANDLER")]
    fn toggle(&self) {
        run_toggle();
    }

    /// Slint rebuilds the window after every hide without a way to pass an activation token, so
    /// the token is not applied yet and focus follows the compositor's policy.
    #[expect(clippy::unused_self, reason = "zbus interface methods take &self; the handler lives in HANDLER")]
    fn toggle_with_token(&self, token: &str) {
        let _ = token;
        run_toggle();
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
        ToggleService.introspect_to_writer(&mut xml, 0);

        assert!(xml.contains(&format!("<method name=\"{DBUS_TOGGLE_METHOD}\"")));
        assert!(xml.contains(&format!("<method name=\"{DBUS_TOGGLE_WITH_TOKEN_METHOD}\"")));
    }
}
