//! Linux session and power calls. Sign out, restart and shut down go through the desktop's session
//! manager (Plasma's `org.kde.Shutdown`, then GNOME's `org.gnome.SessionManager`), so apps can
//! save their work and the session is stored. Lock and sleep, and restart and shut down when no
//! session manager answers, go through logind. Without a session manager Sign out is not offered,
//! because ending the session through logind closes apps without asking them.

use std::sync::OnceLock;
use std::thread;

use anyhow::{Context, Result};
use zbus::blocking::Connection;

use super::PowerAction;
use crate::dbus;

const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";
/// logind's alias for the session of the calling process.
const OWN_SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";

static CAPABILITIES: OnceLock<Capabilities> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionManager {
    Plasma,
    Gnome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Capabilities {
    session_manager: Option<SessionManager>,
    can_suspend: bool,
    can_reboot: bool,
    can_power_off: bool,
}

/// Actions this session can perform, in power flyout order. Probes the buses on first use, so call
/// it off the UI thread.
pub fn available() -> Vec<PowerAction> {
    available_actions(capabilities())
}

/// Runs `action` on a short-lived thread, because the calls block on D-Bus replies.
pub fn perform(action: PowerAction) {
    let spawned = thread::Builder::new().name("rusty-start-power".into()).spawn(move || {
        if let Err(error) = run(action) {
            eprintln!("failed to {}: {error:#}", action.label().to_lowercase());
        }
    });
    if let Err(error) = spawned {
        eprintln!("failed to spawn the power action thread: {error}");
    }
}

fn available_actions(capabilities: Capabilities) -> Vec<PowerAction> {
    PowerAction::ALL
        .into_iter()
        .filter(|action| match action {
            PowerAction::Lock => true,
            PowerAction::SignOut => capabilities.session_manager.is_some(),
            PowerAction::Sleep => capabilities.can_suspend,
            PowerAction::Restart => capabilities.can_reboot,
            PowerAction::ShutDown => capabilities.can_power_off,
        })
        .collect()
}

fn capabilities() -> Capabilities {
    *CAPABILITIES.get_or_init(|| {
        let session_manager: Option<SessionManager> = probe_session_manager();
        let logind: Option<Connection> =
            dbus::system().inspect_err(|error| eprintln!("power actions need logind: {error:#}")).ok();
        let can = |method: &str| logind.as_ref().is_some_and(|connection| logind_allows(connection, method));
        Capabilities {
            session_manager,
            can_suspend: can("CanSuspend"),
            can_reboot: session_manager.is_some() || can("CanReboot"),
            can_power_off: session_manager.is_some() || can("CanPowerOff"),
        }
    })
}

fn probe_session_manager() -> Option<SessionManager> {
    let connection: Connection = dbus::session().ok()?;
    let proxy = zbus::blocking::fdo::DBusProxy::new(&connection).ok()?;
    let has_owner = |name: &'static str| {
        zbus::names::BusName::try_from(name)
            .ok()
            .and_then(|bus_name| proxy.name_has_owner(bus_name).ok())
            .unwrap_or(false)
    };
    if has_owner("org.kde.Shutdown") {
        Some(SessionManager::Plasma)
    } else if has_owner("org.gnome.SessionManager") {
        Some(SessionManager::Gnome)
    } else {
        None
    }
}

/// logind answers `Can*` with `yes`, `no`, `challenge` (allowed after authentication) or `na`.
fn logind_allows(connection: &Connection, method: &str) -> bool {
    connection
        .call_method(Some(LOGIND), LOGIND_PATH, Some(LOGIND_MANAGER), method, &())
        .ok()
        .and_then(|reply| reply.body().deserialize::<String>().ok())
        .is_some_and(|answer| answer == "yes" || answer == "challenge")
}

fn run(action: PowerAction) -> Result<()> {
    let session_manager: Option<SessionManager> = capabilities().session_manager;
    match (action, session_manager) {
        (PowerAction::Lock, _) => call_system(OWN_SESSION_PATH, "org.freedesktop.login1.Session", "Lock", None),
        (PowerAction::Sleep, _) => call_system(LOGIND_PATH, LOGIND_MANAGER, "Suspend", Some(true)),
        (PowerAction::SignOut, Some(SessionManager::Plasma)) => call_plasma("logout"),
        (PowerAction::Restart, Some(SessionManager::Plasma)) => call_plasma("logoutAndReboot"),
        (PowerAction::ShutDown, Some(SessionManager::Plasma)) => call_plasma("logoutAndShutdown"),
        // Mode 1 logs out without GNOME's confirmation dialog; the menu already asked.
        (PowerAction::SignOut, Some(SessionManager::Gnome)) => call_gnome("Logout", Some(1)),
        (PowerAction::Restart, Some(SessionManager::Gnome)) => call_gnome("RequestReboot", None),
        (PowerAction::ShutDown, Some(SessionManager::Gnome)) => call_gnome("RequestShutdown", None),
        (PowerAction::SignOut, None) => anyhow::bail!("no session manager is available"),
        (PowerAction::Restart, None) => call_system(LOGIND_PATH, LOGIND_MANAGER, "Reboot", Some(true)),
        (PowerAction::ShutDown, None) => call_system(LOGIND_PATH, LOGIND_MANAGER, "PowerOff", Some(true)),
    }
}

/// Calls a logind method. `interactive` is the flag logind's power methods take, which lets polkit
/// ask for authentication; methods without arguments pass `None`.
fn call_system(path: &str, interface: &str, method: &str, interactive: Option<bool>) -> Result<()> {
    let connection: Connection = dbus::system()?;
    let reply = match interactive {
        Some(interactive) => connection.call_method(Some(LOGIND), path, Some(interface), method, &(interactive,)),
        None => connection.call_method(Some(LOGIND), path, Some(interface), method, &()),
    };
    reply.with_context(|| format!("logind {method} failed"))?;
    Ok(())
}

fn call_plasma(method: &str) -> Result<()> {
    dbus::session()?
        .call_method(Some("org.kde.Shutdown"), "/Shutdown", Some("org.kde.Shutdown"), method, &())
        .with_context(|| format!("Plasma's {method} failed"))?;
    Ok(())
}

/// Calls a GNOME session manager method; `mode` is `Logout`'s only argument.
fn call_gnome(method: &str, mode: Option<u32>) -> Result<()> {
    let connection: Connection = dbus::session()?;
    let (service, path) = ("org.gnome.SessionManager", "/org/gnome/SessionManager");
    let reply = match mode {
        Some(mode) => connection.call_method(Some(service), path, Some(service), method, &(mode,)),
        None => connection.call_method(Some(service), path, Some(service), method, &()),
    };
    reply.with_context(|| format!("GNOME's {method} failed"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERYTHING: Capabilities = Capabilities {
        session_manager: Some(SessionManager::Plasma),
        can_suspend: true,
        can_reboot: true,
        can_power_off: true,
    };

    #[test]
    fn a_full_session_offers_every_action_in_flyout_order() {
        assert_eq!(available_actions(EVERYTHING), PowerAction::ALL);
    }

    #[test]
    fn sign_out_is_hidden_without_a_session_manager() {
        let actions = available_actions(Capabilities { session_manager: None, ..EVERYTHING });
        assert_eq!(actions, [PowerAction::Lock, PowerAction::Sleep, PowerAction::Restart, PowerAction::ShutDown]);
    }

    #[test]
    fn actions_logind_refuses_are_hidden() {
        let actions = available_actions(Capabilities { can_suspend: false, can_power_off: false, ..EVERYTHING });
        assert_eq!(actions, [PowerAction::Lock, PowerAction::SignOut, PowerAction::Restart]);
    }
}
