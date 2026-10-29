//! Identifiers shared by the host process and other processes that toggle the menu.
//!
//! On Windows a sender signals the host by posting a registered window message to the host's
//! hidden trigger window. On Linux the host serves a D-Bus interface on the session bus. Both sides
//! resolve the names below at runtime.

/// Application ID on Linux: the host's well-known session bus name, its Wayland app ID and the
/// base name of its `.desktop` file. KWin window rules and restricted-interface authorization key
/// off it.
pub const APP_ID: &str = "io.github.jav76.RustyStart";

/// Object path of the host's D-Bus interface.
pub const DBUS_OBJECT_PATH: &str = "/io/github/jav76/RustyStart";

/// D-Bus interface the host serves at `DBUS_OBJECT_PATH`.
///
/// - `Toggle()` toggles the menu. Desktop bindings that can call D-Bus methods use it.
/// - `ToggleWithToken(s token)` toggles the menu and uses `token` as the xdg-activation token, so
///   the menu can take keyboard focus. A second `rusty-start` launch passes its
///   `XDG_ACTIVATION_TOKEN`, or an empty string when it has none.
pub const DBUS_INTERFACE: &str = "io.github.jav76.RustyStart";

/// kglobalaccel action ID of the global shortcut that toggles the menu on KDE Plasma:
/// `[component, action, component display name, action display name]`. `rusty-start setup` binds
/// the Meta key to it; the host registers it and listens for presses.
pub const GLOBAL_SHORTCUT_ACTION_ID: [&str; 4] = [APP_ID, "toggle", "Rusty Start", "Toggle the menu"];

/// `Toggle()` method of `DBUS_INTERFACE`.
pub const DBUS_TOGGLE_METHOD: &str = "Toggle";

/// `ToggleWithToken(s token)` method of `DBUS_INTERFACE`.
pub const DBUS_TOGGLE_WITH_TOKEN_METHOD: &str = "ToggleWithToken";

/// Window class of the host's hidden trigger window.
///
/// Senders locate the window with `FindWindowW(MESSAGE_WINDOW_CLASS, None)`.
pub const MESSAGE_WINDOW_CLASS: &str = "RustyStart.TriggerWindow";

/// Name passed to `RegisterWindowMessageW` to obtain the "toggle the menu" message ID.
///
/// The message must be posted with `PostMessageW`, never sent with `SendMessageW`:
/// the host reads it from its message queue rather than from a window procedure.
///
/// Senders that own the foreground (a second host launch, a program the user just used) must call
/// `AllowSetForegroundWindow` with the host's process ID before posting. Without it Windows
/// refuses the host's `SetForegroundWindow`, and the menu opens without keyboard focus. The host's
/// process ID comes from `GetWindowThreadProcessId` on the trigger window.
pub const TOGGLE_MESSAGE_NAME: &str = "RustyStart.ToggleMenu";

/// Name of the mutex that keeps one host running per session. A launch that finds it already
/// held toggles the running host's menu and exits.
pub const SINGLE_INSTANCE_MUTEX_NAME: &str = "Local\\RustyStart.Host";
