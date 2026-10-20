//! Session bus connection shared by the Linux backends.
//!
//! One connection owns the host's well-known name and serves its interface, so callers reach the
//! object through the name. zbus runs the connection on its own executor thread.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use zbus::blocking::Connection;

static SESSION: OnceLock<Connection> = OnceLock::new();

/// Returns the shared session bus connection, connecting on first use.
pub fn session() -> Result<Connection> {
    if let Some(connection) = SESSION.get() {
        return Ok(connection.clone());
    }

    let connection: Connection = Connection::session().context("failed to connect to the session bus")?;
    // A racing caller may have connected first; its connection wins and this one is dropped.
    Ok(SESSION.get_or_init(|| connection).clone())
}
