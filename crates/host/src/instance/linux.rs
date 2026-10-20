use std::env;

use anyhow::{Context, Result};
use rusty_start_protocol::{APP_ID, DBUS_INTERFACE, DBUS_OBJECT_PATH, DBUS_TOGGLE_WITH_TOKEN_METHOD};
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::{dbus, trigger};

/// Keeps the host's well-known bus name. The bus releases the name when the connection closes.
pub struct InstanceGuard {
    _connection: Connection,
}

/// Claims `APP_ID` on the session bus. When another host already owns it, toggles that host's
/// menu, forwarding this launch's activation token so the menu can take keyboard focus, and
/// returns `None`.
pub fn acquire() -> Result<Option<InstanceGuard>> {
    let connection: Connection = dbus::session()?;
    // Serve first, so a toggle that arrives the moment the name is ours (the call that started the
    // host through D-Bus activation) is queued rather than refused.
    trigger::serve_toggle_interface(&connection)?;
    match connection.request_name_with_flags(APP_ID, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
            Ok(Some(InstanceGuard { _connection: connection }))
        }
        // zbus reports a name held by another connection as `NameTaken` rather than `Exists`.
        Ok(RequestNameReply::Exists | RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => {
            toggle_running_host(&connection)?;
            Ok(None)
        }
        Err(error) => Err(error).context("failed to request the Rusty Start bus name"),
    }
}

fn toggle_running_host(connection: &Connection) -> Result<()> {
    let token: String = env::var("XDG_ACTIVATION_TOKEN").unwrap_or_default();
    connection
        .call_method(Some(APP_ID), DBUS_OBJECT_PATH, Some(DBUS_INTERFACE), DBUS_TOGGLE_WITH_TOKEN_METHOD, &(token,))
        .context("failed to toggle the running host's menu")?;
    Ok(())
}
