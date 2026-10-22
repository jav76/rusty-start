#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible win32 backend")]

use crate::setup::{ItemState, SetupItem};

/// Nothing else watches the Windows key in preview builds, so the menu window closes on its tap.
pub const MENU_HANDLES_WIN_KEY: bool = true;

/// There is no system Start menu to suppress outside Windows.
pub fn suppress_native_start_menu() {}

/// Placeholder guard so the host's startup code is identical on every platform.
pub struct TriggerGuard;

/// Opens the menu once. The toggle is queued and runs when the event loop starts.
pub fn install(on_toggle: impl Fn() + Send + 'static) -> anyhow::Result<TriggerGuard> {
    on_toggle();
    Ok(TriggerGuard)
}

/// Logs the change; the preview has no Windows-key hook to install.
pub fn set_win_key(enabled: bool) {
    eprintln!("preview: the Windows key would {} the menu", if enabled { "open" } else { "no longer open" });
}

/// Stand-in hook rows, so the Setup section shows read-only status in each state it uses on Windows.
pub fn status_items() -> Vec<SetupItem> {
    vec![
        SetupItem::new("Windows key hook", ItemState::Unavailable, "none in preview builds"),
        SetupItem::new("Taskbar hook", ItemState::Conflict, "sample failure (preview)"),
    ]
}
