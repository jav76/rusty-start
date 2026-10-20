#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible Linux setup")]

use anyhow::Result;

use super::Options;

pub fn apply(_options: Options) -> Result<()> {
    eprintln!("Rusty Start needs no setup on this platform.");
    Ok(())
}

pub fn undo() -> Result<()> {
    eprintln!("Rusty Start made no setup changes on this platform.");
    Ok(())
}
