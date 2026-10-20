//! Identifiers shared by the host process and the explorer hook.
//!
//! The hook signals the host by posting a registered window message to the host's hidden
//! trigger window. Both sides resolve the names below at runtime.

/// Window class of the host's hidden trigger window.
///
/// The hook locates the window with `FindWindowW(MESSAGE_WINDOW_CLASS, None)`.
pub const MESSAGE_WINDOW_CLASS: &str = "RustyStart.TriggerWindow";

/// Name passed to `RegisterWindowMessageW` to obtain the "toggle the menu" message ID.
///
/// The message must be posted with `PostMessageW`, never sent with `SendMessageW`:
/// the host reads it from its message queue rather than from a window procedure.
///
/// Senders that own the foreground (the explorer hook, a second host launch) must call
/// `AllowSetForegroundWindow` with the host's process ID before posting. Without it Windows
/// refuses the host's `SetForegroundWindow`, and the menu opens without keyboard focus. The host's
/// process ID comes from `GetWindowThreadProcessId` on the trigger window.
pub const TOGGLE_MESSAGE_NAME: &str = "RustyStart.ToggleMenu";

/// Name of the mutex that keeps one host running per session. A launch that finds it already
/// held toggles the running host's menu and exits.
pub const SINGLE_INSTANCE_MUTEX_NAME: &str = "Local\\RustyStart.Host";

/// Name of the named shared memory section for exchanging geometry between host and hook.
pub const HOOK_SHARED_MEMORY_NAME: &str = "Local\\RustyStartHookShared";

/// Name of the exported hook procedure in `rusty_start_hook.dll`.
pub const HOOK_PROC_NAME: &str = "rusty_start_hook_proc";

/// File name of the compiled hook dynamic library.
pub const HOOK_DLL_NAME: &str = "rusty_start_hook.dll";

/// A 2D integer rectangle matching Win32 `RECT` layout without a heavy dependency.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    /// Creates a new rectangle.
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    /// Checks if a physical screen point `(x, y)` lies inside this rectangle.
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// Checks if this rectangle has positive area.
    pub const fn is_valid(&self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
}

/// Shared memory layout exchanged between `rusty-start-host` and `rusty_start_hook.dll`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HookSharedState {
    /// Bounding rectangle of the primary taskbar Start button.
    pub primary_start_rect: Rect,
    /// Bounding rectangle of any secondary taskbar Start button.
    pub secondary_start_rect: Rect,
    /// Whether a secondary taskbar Start button is present and tracked.
    pub has_secondary: bool,
    /// Whether the hook is actively intercepting clicks.
    pub enabled: bool,
    /// HWND handle of the host's trigger window as a raw integer.
    pub trigger_hwnd: usize,
    /// Registered message ID for `RustyStart.ToggleMenu`.
    pub toggle_msg_id: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_points_inside_and_excludes_points_outside() {
        let r = Rect::new(10, 20, 50, 60);
        assert!(r.contains(10, 20));
        assert!(r.contains(49, 59));
        assert!(!r.contains(9, 20));
        assert!(!r.contains(50, 20));
        assert!(!r.contains(10, 60));
        assert!(!r.contains(10, 19));
    }

    #[test]
    fn rect_validity_requires_positive_dimensions() {
        assert!(Rect::new(0, 0, 10, 10).is_valid());
        assert!(!Rect::new(10, 0, 10, 10).is_valid());
        assert!(!Rect::new(0, 10, 10, 10).is_valid());
        assert!(!Rect::new(10, 10, 0, 0).is_valid());
    }

    #[test]
    fn hook_shared_state_is_consistently_aligned() {
        assert!(std::mem::size_of::<HookSharedState>() >= 40);
        assert_eq!(std::mem::align_of::<HookSharedState>(), std::mem::align_of::<usize>());
    }
}
