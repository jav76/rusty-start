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
pub const TOGGLE_MESSAGE_NAME: &str = "RustyStart.ToggleMenu";
