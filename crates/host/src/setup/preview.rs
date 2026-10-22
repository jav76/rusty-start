//! Preview setup: in-memory state standing in for desktop integration, so the Settings Page's
//! Setup section can be exercised on any OS without changing the desktop.

#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible setup backends")]

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};

use super::{ItemState, MetaKeyState, Options, SetupItem};

/// The preview keeps the Windows key in `use_win_key`, like Windows.
pub const OWNS_META_KEY: bool = false;

static AUTOSTART_INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn apply(_options: Options, changes: &mut Vec<String>) -> Result<()> {
    if AUTOSTART_INSTALLED.swap(true, Ordering::Relaxed) {
        changes.push("Autostart: already installed (preview, nothing written)".to_owned());
    } else {
        changes.push("Autostart: installed (preview, nothing written)".to_owned());
    }
    Ok(())
}

pub fn undo(_options: Options, changes: &mut Vec<String>) -> Result<()> {
    if AUTOSTART_INSTALLED.swap(false, Ordering::Relaxed) {
        changes.push("Autostart: removed (preview, nothing written)".to_owned());
    }
    Ok(())
}

pub fn audit() -> Result<Vec<SetupItem>> {
    let state: ItemState =
        if AUTOSTART_INSTALLED.load(Ordering::Relaxed) { ItemState::Installed } else { ItemState::Missing };
    let detail: &str = if state == ItemState::Installed { "installed (preview)" } else { "not installed (preview)" };
    Ok(vec![SetupItem::new("Autostart", state, detail)])
}

/// Never called: the preview keeps the Windows key in `use_win_key` (see `OWNS_META_KEY`).
pub fn meta_key() -> Result<MetaKeyState> {
    bail!("the preview keeps the Windows key in config.toml")
}

/// Never called: the preview keeps the Windows key in `use_win_key` (see `OWNS_META_KEY`).
pub fn set_meta_key(_enabled: bool, _changes: &mut Vec<String>) -> Result<()> {
    bail!("the preview keeps the Windows key in config.toml")
}
