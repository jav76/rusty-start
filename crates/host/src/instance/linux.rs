use std::env;

use anyhow::{Context, Result};
use rusty_start_protocol::{APP_ID, DBUS_INTERFACE, DBUS_OBJECT_PATH, DBUS_QUIT_METHOD, DBUS_TOGGLE_WITH_TOKEN_METHOD};
use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::names::BusName;

use super::HostRequest;
use crate::{dbus, trigger};

/// Keeps the host's well-known bus name. The bus releases the name when the connection closes.
pub struct InstanceGuard {
    _connection: Connection,
}

/// Claims `APP_ID` on the session bus. Returns `None` when another host already owns it.
pub fn acquire() -> Result<Option<InstanceGuard>> {
    let connection: Connection = dbus::session()?;
    // Serve first, so a toggle that arrives the moment the name is ours (the call that started the
    // host through D-Bus activation) is queued rather than refused.
    trigger::serve_host_interface(&connection)?;
    match connection.request_name_with_flags(APP_ID, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
            Ok(Some(InstanceGuard { _connection: connection }))
        }
        // zbus reports a name held by another connection as `NameTaken` rather than `Exists`.
        Ok(RequestNameReply::Exists | RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => Ok(None),
        Err(error) => Err(error).context("failed to request the Rusty Start bus name"),
    }
}

/// Calls the running host over the session bus. A toggle forwards this launch's activation token
/// so the menu can take keyboard focus, and starts a host through D-Bus activation when none runs,
/// so it always returns `true`. A quit returns `false` when no host owns the bus name.
pub fn send(request: HostRequest) -> Result<bool> {
    let connection: Connection = dbus::session()?;
    match request {
        HostRequest::Toggle => {
            let token: String = env::var("XDG_ACTIVATION_TOKEN").unwrap_or_default();
            connection
                .call_method(
                    Some(APP_ID),
                    DBUS_OBJECT_PATH,
                    Some(DBUS_INTERFACE),
                    DBUS_TOGGLE_WITH_TOKEN_METHOD,
                    &(token,),
                )
                .context("failed to toggle the running host's menu")?;
        }
        HostRequest::Quit => {
            if !host_owns_bus_name(&connection)? {
                return Ok(false);
            }
            connection
                .call_method(Some(APP_ID), DBUS_OBJECT_PATH, Some(DBUS_INTERFACE), DBUS_QUIT_METHOD, &())
                .context("failed to ask the running host to quit")?;
        }
    }
    Ok(true)
}

fn host_owns_bus_name(connection: &Connection) -> Result<bool> {
    let bus: DBusProxy<'_> = DBusProxy::new(connection).context("failed to reach the session bus daemon")?;
    let name: BusName<'_> = BusName::try_from(APP_ID).context("the application ID is not a valid bus name")?;
    bus.name_has_owner(name).context("failed to ask whether a host owns the bus name")
}
