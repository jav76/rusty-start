//! Lock, sign out, sleep, restart and shut down, offered in the power flyout and as search results.
//!
//! On Windows they call the session and power APIs. On other platforms they only log, so preview
//! builds never end the developer's session.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::perform;

#[cfg(not(windows))]
mod preview;
#[cfg(not(windows))]
pub use preview::perform;

use crate::search::Searchable;

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
