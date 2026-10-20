//! `rusty-start setup`: one-time desktop integration that the host never does on its own.
//!
//! On Linux it binds the Meta key, adds a KWin window rule, enables autostart and, with `--dev`,
//! authorizes the current executable for KWin screenshots. `--undo` reverses it. Other backends
//! need no setup.

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
use linux::{apply, undo};

#[cfg(not(backend_linux))]
mod other;
#[cfg(not(backend_linux))]
use other::{apply, undo};

#[cfg(any(backend_linux, test))]
mod values;

use anyhow::{Result, bail};

const USAGE: &str = "usage: rusty-start setup [--dev] [--undo]";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Also install a desktop entry for the current executable, so KWin authorizes screenshots
    /// from a development build.
    pub dev: bool,
}

/// Runs `rusty-start setup` with the arguments that follow `setup`.
pub fn run(arguments: &[String]) -> Result<()> {
    let mut options = Options::default();
    let mut reverse: bool = false;
    for argument in arguments {
        match argument.as_str() {
            "--dev" => options.dev = true,
            "--undo" => reverse = true,
            "--help" | "-h" => {
                eprintln!("{USAGE}");
                return Ok(());
            }
            other => bail!("unknown setup option {other:?}\n{USAGE}"),
        }
    }

    if reverse { undo() } else { apply(options) }
}
