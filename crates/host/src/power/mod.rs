//! Lock, sign out, sleep, restart and shut down, offered in the power flyout and as search results.
//!
//! On Windows they call the session and power APIs; on Linux the desktop's session manager and
//! logind. Preview builds only log, so they never end the developer's session.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::available;
#[cfg(backend_win32)]
use win32::run;

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::available;
#[cfg(backend_linux)]
use linux::run;

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::available;
#[cfg(backend_preview)]
use preview::run;

use std::thread;

use crate::persist;
use crate::search::Searchable;

/// Runs `action` on a short-lived thread, because the platform calls block (suspend returns only
/// when the computer wakes, D-Bus calls wait for replies). Actions that end the session first
/// write every pending save.
pub fn perform(action: PowerAction) {
    let spawned = thread::Builder::new().name("rusty-start-power".into()).spawn(move || {
        if action.ends_session() {
            persist::flush_all();
        }
        if let Err(error) = run(action) {
            log::error!("failed to {}: {error:#}", action.label().to_lowercase());
        }
    });
    if let Err(error) = spawned {
        log::error!("failed to spawn the power action thread: {error}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    SignOut,
    Sleep,
    Restart,
    ShutDown,
}

impl PowerAction {
    /// Power flyout order, matching the built-in Start menu.
    pub const ALL: [Self; 5] = [Self::Lock, Self::SignOut, Self::Sleep, Self::Restart, Self::ShutDown];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lock => "Lock",
            Self::SignOut => "Sign out",
            Self::Sleep => "Sleep",
            Self::Restart => "Restart",
            Self::ShutDown => "Shut down",
        }
    }

    /// Actions that close apps ask for a second selection when picked from search results, where
    /// a stray Enter is easy. The power flyout is already a deliberate two-click path.
    pub fn needs_confirmation(self) -> bool {
        self.ends_session()
    }

    /// Actions after which the host does not get to finish its background writes.
    pub fn ends_session(self) -> bool {
        matches!(self, Self::SignOut | Self::Restart | Self::ShutDown)
    }

    pub fn confirmation_prompt(self) -> &'static str {
        match self {
            Self::Lock => "Select again to lock",
            Self::SignOut => "Select again to sign out",
            Self::Sleep => "Select again to sleep",
            Self::Restart => "Select again to restart",
            Self::ShutDown => "Select again to shut down",
        }
    }
}

impl Searchable for PowerAction {
    fn name(&self) -> &str {
        self.label()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_actions_that_end_the_session_need_confirmation_and_a_flush() {
        for action in PowerAction::ALL {
            let ends: bool = matches!(action, PowerAction::SignOut | PowerAction::Restart | PowerAction::ShutDown);
            assert_eq!(action.ends_session(), ends, "{action:?}");
            assert_eq!(action.needs_confirmation(), ends, "{action:?}");
        }
    }
}
