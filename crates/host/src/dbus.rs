//! Bus connections shared by the Linux backends.
//!
//! One session connection owns the host's well-known name and serves its interface, so callers
//! reach the object through the name. zbus runs each connection on its own executor thread.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use zbus::blocking::Connection;

static SESSION: OnceLock<Connection> = OnceLock::new();
static SYSTEM: OnceLock<Connection> = OnceLock::new();

/// Returns the shared session bus connection, connecting on first use.
pub fn session() -> Result<Connection> {
    shared(&SESSION, Connection::session).context("failed to connect to the session bus")
}

/// Returns the shared system bus connection (logind), connecting on first use.
pub fn system() -> Result<Connection> {
    shared(&SYSTEM, Connection::system).context("failed to connect to the system bus")
}

fn shared(cell: &OnceLock<Connection>, connect: fn() -> zbus::Result<Connection>) -> zbus::Result<Connection> {
    if let Some(connection) = cell.get() {
        return Ok(connection.clone());
    }
    let connection: Connection = connect()?;
    // A racing caller may have connected first; its connection wins and this one is dropped.
    Ok(cell.get_or_init(|| connection).clone())
}
