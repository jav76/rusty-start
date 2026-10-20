//! Decides when the menu opens.
//!
//! On Windows a hidden window listens for the Windows key, a development hotkey and the toggle
//! message other processes post. On Linux the host serves a D-Bus interface that compositor
//! bindings and second launches call. Preview builds have no global trigger, so the menu opens
//! once at startup to allow UI iteration.

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{MENU_HANDLES_WIN_KEY, TriggerGuard, install, suppress_native_start_menu};

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::{MENU_HANDLES_WIN_KEY, TriggerGuard, install, suppress_native_start_menu};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{MENU_HANDLES_WIN_KEY, TriggerGuard, install, suppress_native_start_menu};

/// A key event seen by the menu window while it has keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    WinPressed,
    WinReleased,
    /// Any other key press, except the mask key itself.
    OtherPressed,
}

/// What the menu window must do after a key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKeyAction {
    None,
    /// The Windows key just went down: mask it now, so its release cannot open the built-in
    /// Start menu.
    Mask,
    /// The Windows key was tapped alone: close the menu.
    Toggle,
}

/// Tracks a Windows-key tap seen by the menu window. While the menu has keyboard focus the
/// low-level hook does not see its keys, so the window detects the closing tap itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuWinKeyTap {
    held: bool,
    /// No other key was pressed since the Windows key went down.
    solo: bool,
}

impl MenuWinKeyTap {
    pub fn on_key(&mut self, key: MenuKey) -> MenuKeyAction {
        match key {
            // Auto-repeat sends more presses while the key is held; only the first one counts.
            MenuKey::WinPressed if self.held => MenuKeyAction::None,
            MenuKey::WinPressed => {
                *self = Self { held: true, solo: true };
                MenuKeyAction::Mask
            }
            MenuKey::WinReleased => {
                let was_solo_tap: bool = self.held && self.solo;
                *self = Self::default();
                if was_solo_tap { MenuKeyAction::Toggle } else { MenuKeyAction::None }
            }
            MenuKey::OtherPressed => {
                self.solo = false;
                MenuKeyAction::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(keys: &[MenuKey]) -> Vec<MenuKeyAction> {
        let mut tap = MenuWinKeyTap::default();
        keys.iter().map(|&key| tap.on_key(key)).collect()
    }

    #[test]
    fn masks_on_press_and_toggles_on_a_solo_release() {
        assert_eq!(run(&[MenuKey::WinPressed, MenuKey::WinReleased]), [MenuKeyAction::Mask, MenuKeyAction::Toggle]);
    }

    #[test]
    fn a_chord_masks_but_does_not_toggle() {
        assert_eq!(
            run(&[MenuKey::WinPressed, MenuKey::OtherPressed, MenuKey::WinReleased]),
            [MenuKeyAction::Mask, MenuKeyAction::None, MenuKeyAction::None]
        );
    }

    #[test]
    fn auto_repeat_masks_only_once() {
        assert_eq!(
            run(&[MenuKey::WinPressed, MenuKey::WinPressed, MenuKey::WinReleased]),
            [MenuKeyAction::Mask, MenuKeyAction::None, MenuKeyAction::Toggle]
        );
    }

    #[test]
    fn a_release_without_a_press_does_nothing() {
        assert_eq!(run(&[MenuKey::WinReleased]), [MenuKeyAction::None]);
    }

    #[test]
    fn keys_before_the_press_do_not_break_the_next_tap() {
        assert_eq!(
            run(&[MenuKey::OtherPressed, MenuKey::WinPressed, MenuKey::WinReleased]),
            [MenuKeyAction::None, MenuKeyAction::Mask, MenuKeyAction::Toggle]
        );
    }
}
