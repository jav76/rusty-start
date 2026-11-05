//! Detects a solo Windows-key tap from the low-level keyboard events the Windows backend sees.
//! Kept free of Win32 types so its tests run on every platform, including the Linux CI runner.
//!
//! A tap counts only when the key goes down with no modifier held and comes up with no other key
//! pressed in between, so every `Win+<Key>` shortcut keeps working. Key releases can go missing:
//! Win+L and a switch to the secure desktop (UAC, Ctrl+Alt+Del) take the keyboard away before the
//! release is seen. The tracker therefore treats a Windows key reported held for longer than
//! [`STALE_HOLD`] as released, and reads the live modifier state on every fresh press instead of
//! trusting what it saw earlier.

use std::time::{Duration, Instant};

pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;

/// A Windows key held longer than this had its release lost; auto-repeat arrives every few tens of
/// milliseconds, so a real hold never looks stale between repeats.
pub const STALE_HOLD: Duration = Duration::from_secs(2);

/// Modifier virtual keys and the bit each one occupies in the held-modifier mask.
const MODIFIERS: [(u16, u16); 9] = [
    (0x10, 1 << 0), // VK_SHIFT
    (0x11, 1 << 1), // VK_CONTROL
    (0x12, 1 << 2), // VK_MENU (Alt)
    (0xA0, 1 << 3), // VK_LSHIFT
    (0xA1, 1 << 4), // VK_RSHIFT
    (0xA2, 1 << 5), // VK_LCONTROL
    (0xA3, 1 << 6), // VK_RCONTROL
    (0xA4, 1 << 7), // VK_LMENU
    (0xA5, 1 << 8), // VK_RMENU
];

/// Tracks whether `VK_LWIN` or `VK_RWIN` was tapped alone without any modifier or chord key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WinKeyChord {
    win_held: bool,
    chord_interrupted: bool,
    held_modifiers: u16,
    win_down_at: Option<Instant>,
}

impl WinKeyChord {
    pub const fn new() -> Self {
        Self { win_held: false, chord_interrupted: false, held_modifiers: 0, win_down_at: None }
    }

    /// Processes a non-injected key event and returns `true` when a solo Windows key tap completes
    /// on key release. `live_modifiers` reads the modifier mask from the keyboard itself and is
    /// consulted once per fresh Windows-key press.
    pub fn on_key_event(
        &mut self,
        vk: u16,
        is_key_up: bool,
        now: Instant,
        live_modifiers: impl FnOnce() -> u16,
    ) -> bool {
        if vk == VK_LWIN || vk == VK_RWIN {
            if is_key_up {
                let should_toggle: bool = self.win_held && !self.chord_interrupted;
                self.win_held = false;
                self.chord_interrupted = false;
                self.win_down_at = None;
                should_toggle
            } else {
                if !self.win_held || self.hold_is_stale(now) {
                    self.win_held = true;
                    self.win_down_at = Some(now);
                    self.held_modifiers = live_modifiers();
                    self.chord_interrupted = self.held_modifiers != 0;
                }
                false
            }
        } else {
            if let Some(bit) = modifier_bit(vk) {
                if is_key_up {
                    self.held_modifiers &= !bit;
                } else {
                    self.held_modifiers |= bit;
                }
            }

            if self.win_held {
                self.chord_interrupted = true;
            }
            false
        }
    }

    /// Forgets everything, for when the session was locked or unlocked and key releases were lost.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn hold_is_stale(&self, now: Instant) -> bool {
        self.win_down_at.is_some_and(|down_at| now.saturating_duration_since(down_at) > STALE_HOLD)
    }
}

/// Builds the held-modifier mask from a keyboard query such as `GetAsyncKeyState`.
pub fn modifiers_held(is_down: impl Fn(u16) -> bool) -> u16 {
    MODIFIERS.iter().filter(|(vk, _)| is_down(*vk)).fold(0, |mask, (_, bit)| mask | bit)
}

const fn modifier_bit(vk: u16) -> Option<u16> {
    let mut index: usize = 0;
    while index < MODIFIERS.len() {
        if MODIFIERS[index].0 == vk {
            return Some(MODIFIERS[index].1);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_R: u16 = 0x52;
    const KEY_L: u16 = 0x4C;
    const LCTRL: u16 = 0xA2;
    const LCTRL_BIT: u16 = 1 << 5;

    /// Sends an event with the keyboard reporting no modifier held.
    fn key(chord: &mut WinKeyChord, vk: u16, is_key_up: bool, now: Instant) -> bool {
        chord.on_key_event(vk, is_key_up, now, || 0)
    }

    #[test]
    fn solo_windows_key_tap_triggers_on_release() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!key(&mut chord, VK_LWIN, false, now));
        assert!(key(&mut chord, VK_LWIN, true, now));
    }

    #[test]
    fn auto_repeated_solo_windows_key_still_triggers_on_release() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!key(&mut chord, VK_RWIN, false, now));
        assert!(!key(&mut chord, VK_RWIN, false, now + Duration::from_millis(30)));
        assert!(key(&mut chord, VK_RWIN, true, now + Duration::from_millis(60)));
    }

    #[test]
    fn win_plus_key_chord_does_not_trigger_menu() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!key(&mut chord, VK_LWIN, false, now));
        assert!(!key(&mut chord, KEY_R, false, now));
        assert!(!key(&mut chord, KEY_R, true, now));
        assert!(!key(&mut chord, VK_LWIN, true, now));
    }

    #[test]
    fn holding_modifier_before_pressing_win_does_not_trigger_menu() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!key(&mut chord, LCTRL, false, now));
        assert!(!chord.on_key_event(VK_LWIN, false, now, || LCTRL_BIT));
        assert!(!key(&mut chord, VK_LWIN, true, now));
        assert!(!key(&mut chord, LCTRL, true, now));

        // Subsequent solo tap after releasing Ctrl works normally.
        assert!(!key(&mut chord, VK_LWIN, false, now));
        assert!(key(&mut chord, VK_LWIN, true, now));
    }

    #[test]
    fn lost_key_up_after_win_l_does_not_block_the_next_tap() {
        let mut chord = WinKeyChord::new();
        let locked_at = Instant::now();

        // Win+L: the lock screen takes the keyboard before either release is seen.
        assert!(!key(&mut chord, VK_LWIN, false, locked_at));
        assert!(!key(&mut chord, KEY_L, false, locked_at));

        let after_unlock = locked_at + Duration::from_secs(30);
        assert!(!key(&mut chord, VK_LWIN, false, after_unlock));
        assert!(key(&mut chord, VK_LWIN, true, after_unlock + Duration::from_millis(80)));
    }

    #[test]
    fn a_hold_within_the_stale_window_is_still_the_same_press() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!key(&mut chord, VK_LWIN, false, now));
        assert!(!key(&mut chord, KEY_R, false, now));
        assert!(!key(&mut chord, KEY_R, true, now));
        // Auto-repeat of the still-held Windows key just inside the window keeps the chord.
        assert!(!key(&mut chord, VK_LWIN, false, now + STALE_HOLD));
        assert!(!key(&mut chord, VK_LWIN, true, now + STALE_HOLD + Duration::from_millis(10)));
    }

    #[test]
    fn modifier_held_before_lock_is_cleared_by_the_live_state() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        // Ctrl+Alt+Del: the releases happen on the secure desktop and are never seen.
        assert!(!key(&mut chord, LCTRL, false, now));
        assert!(!key(&mut chord, 0xA4, false, now));

        let later = now + Duration::from_secs(5);
        assert!(!key(&mut chord, VK_LWIN, false, later));
        assert!(key(&mut chord, VK_LWIN, true, later));
    }

    #[test]
    fn the_live_state_also_blocks_a_tap_the_tracker_missed_the_press_for() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!chord.on_key_event(VK_LWIN, false, now, || LCTRL_BIT));
        assert!(!key(&mut chord, VK_LWIN, true, now));
    }

    #[test]
    fn reset_clears_a_hold_in_progress() {
        let mut chord = WinKeyChord::new();
        let now = Instant::now();

        assert!(!key(&mut chord, VK_LWIN, false, now));
        assert!(!key(&mut chord, KEY_R, false, now));
        chord.reset();

        assert_eq!(chord, WinKeyChord::new());
        assert!(!key(&mut chord, VK_LWIN, true, now));
        assert!(!key(&mut chord, VK_LWIN, false, now));
        assert!(key(&mut chord, VK_LWIN, true, now));
    }

    #[test]
    fn the_modifier_mask_comes_from_the_keyboard_query() {
        assert_eq!(modifiers_held(|_| false), 0);
        assert_eq!(modifiers_held(|vk| vk == LCTRL), LCTRL_BIT);
        assert_eq!(modifiers_held(|vk| vk == 0x10 || vk == 0xA5), (1 << 0) | (1 << 8));
    }
}
