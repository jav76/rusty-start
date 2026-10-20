//! Windows trigger: a hidden window and low-level keyboard hook on a dedicated thread.
//!
//! The window is top-level rather than message-only (`HWND_MESSAGE`) because only top-level
//! windows receive the `TaskbarCreated` broadcast, which the host will need to re-inject the
//! hook after explorer restarts.
//!
//! When `use_win_key` is enabled in `config.toml` (the default), the trigger thread also installs a
//! `WH_KEYBOARD_LL` hook that opens `rusty-start` on a solo Windows-key tap (`VK_LWIN` / `VK_RWIN`
//! released without any other key pressed) while preserving all `Win+<Key>` system shortcuts.

use std::cell::Cell;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};
use rusty_start_protocol::{MESSAGE_WINDOW_CLASS, TOGGLE_MESSAGE_NAME};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    RegisterHotKey, SendInput, UnregisterHotKey, VK_LWIN, VK_NONAME, VK_RWIN, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_UP, MSG, PostMessageW, PostThreadMessageW, RegisterClassExW,
    RegisterWindowMessageW, SetWindowsHookExW, UnhookWindowsHookEx, UnregisterClassW, WH_KEYBOARD_LL, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_DISPLAYCHANGE, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SETTINGCHANGE, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WNDCLASSEXW,
};
use windows::core::{HSTRING, PCWSTR};

use crate::config::Config;
use crate::taskbar::TaskbarHookGuard;

/// The low-level hook does not see keys while the menu has keyboard focus, so the menu window
/// detects the closing Windows-key tap itself.
pub const MENU_HANDLES_WIN_KEY: bool = true;

/// Identifies the fallback development hotkey, Ctrl+Alt+Space.
const DEV_HOTKEY_ID: i32 = 1;

thread_local! {
    static HOOK_STATE: Cell<HookState> = const { Cell::new(HookState::new()) };
}

#[derive(Clone, Copy)]
struct HookState {
    hwnd: HWND,
    toggle_message: u32,
    chord: WinKeyChord,
}

impl HookState {
    const fn new() -> Self {
        Self { hwnd: HWND(std::ptr::null_mut()), toggle_message: 0, chord: WinKeyChord::new() }
    }
}

/// Tracks whether `VK_LWIN` or `VK_RWIN` was tapped alone without any modifier or chord key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WinKeyChord {
    win_held: bool,
    chord_interrupted: bool,
    held_modifiers: u16,
}

impl WinKeyChord {
    const fn new() -> Self {
        Self { win_held: false, chord_interrupted: false, held_modifiers: 0 }
    }

    /// Processes a non-injected key event and returns `true` when a solo Windows key tap completes
    /// on key release.
    fn on_key_event(&mut self, vk: u16, is_key_up: bool) -> bool {
        if vk == VK_LWIN.0 || vk == VK_RWIN.0 {
            if !is_key_up {
                if !self.win_held {
                    self.win_held = true;
                    self.chord_interrupted = self.held_modifiers != 0;
                }
                false
            } else {
                let should_toggle: bool = self.win_held && !self.chord_interrupted;
                self.win_held = false;
                self.chord_interrupted = false;
                should_toggle
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
}

const fn modifier_bit(vk: u16) -> Option<u16> {
    match vk {
        0x10 => Some(1 << 0), // VK_SHIFT
        0x11 => Some(1 << 1), // VK_CONTROL
        0x12 => Some(1 << 2), // VK_MENU (Alt)
        0xA0 => Some(1 << 3), // VK_LSHIFT
        0xA1 => Some(1 << 4), // VK_RSHIFT
        0xA2 => Some(1 << 5), // VK_LCONTROL
        0xA3 => Some(1 << 6), // VK_RCONTROL
        0xA4 => Some(1 << 7), // VK_LMENU
        0xA5 => Some(1 << 8), // VK_RMENU
        _ => None,
    }
}

/// Keeps the trigger thread running. Dropping it stops the thread and releases the hotkey and hook.
pub struct TriggerGuard {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Drop for TriggerGuard {
    fn drop(&mut self) {
        // SAFETY: PostThreadMessageW has no memory-safety preconditions.
        let posted = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };

        // Joining a thread that was never told to quit would block forever.
        if posted.is_ok()
            && let Some(thread) = self.thread.take()
        {
            let _ = thread.join();
        }
    }
}

/// Starts the trigger thread. `on_toggle` runs on that thread, not on the UI thread.
pub fn install(on_toggle: impl Fn() + Send + 'static) -> Result<TriggerGuard> {
    let (ready_sender, ready_receiver) = mpsc::channel::<Result<u32>>();

    let thread: JoinHandle<()> = thread::Builder::new()
        .name("rusty-start-trigger".into())
        .spawn(move || run_trigger_thread(on_toggle, &ready_sender))
        .context("failed to spawn the trigger thread")?;

    match ready_receiver.recv() {
        Ok(Ok(thread_id)) => Ok(TriggerGuard { thread_id, thread: Some(thread) }),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            bail!("the trigger thread exited during startup")
        }
    }
}

fn run_trigger_thread(on_toggle: impl Fn(), ready: &mpsc::Sender<Result<u32>>) {
    let mut window: TriggerWindow = match TriggerWindow::create() {
        Ok(window) => window,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    // Reported only now because a thread has no message queue until it creates a window,
    // and TriggerGuard needs that queue to deliver WM_QUIT.
    // SAFETY: GetCurrentThreadId has no preconditions.
    let thread_id: u32 = unsafe { GetCurrentThreadId() };
    let _ = ready.send(Ok(thread_id));

    run_message_loop(&mut window, on_toggle);
}

fn run_message_loop(window: &mut TriggerWindow, on_toggle: impl Fn()) {
    let mut message = MSG::default();

    loop {
        // SAFETY: `message` is a valid out-parameter. A `None` window filter receives every
        // message for this thread, including the WM_QUIT posted by TriggerGuard.
        let result = unsafe { GetMessageW(&raw mut message, None, 0, 0) };

        // 0 means WM_QUIT was received and -1 means the call failed; both end the loop.
        if result.0 <= 0 {
            break;
        }

        if window.is_toggle_request(&message) {
            on_toggle();
            continue;
        }

        if window.taskbar_created_msg != 0 && message.message == window.taskbar_created_msg {
            // Explorer restarted: re-install the taskbar hook on the new taskbar thread.
            window.reinstall_taskbar_hook();
            continue;
        }

        if message.message == WM_DISPLAYCHANGE || message.message == WM_SETTINGCHANGE {
            // Screen resolution or theme changed: update Start button bounding rects.
            window.update_taskbar_geometry();
        }

        // SAFETY: `message` was filled in by the GetMessageW call above.
        unsafe { DispatchMessageW(&raw const message) };
    }
}

/// Hidden window that owns the hotkey, optional `WH_KEYBOARD_LL` Windows-key hook, and the Explorer
/// taskbar hook. Must stay on the thread that created it.
struct TriggerWindow {
    hwnd: HWND,
    toggle_message: u32,
    taskbar_created_msg: u32,
    keyboard_hook: Option<HHOOK>,
    taskbar_hook: Option<TaskbarHookGuard>,
    // Declared last so the class is unregistered only after Drop has destroyed the window.
    _class: WindowClass,
}

impl TriggerWindow {
    fn create() -> Result<Self> {
        let toggle_message: u32 = register_toggle_message()?;
        let taskbar_created_msg: u32 = register_taskbar_created_message();
        let class: WindowClass = WindowClass::register(MESSAGE_WINDOW_CLASS)?;
        let hwnd: HWND = create_hidden_window(&class)?;

        HOOK_STATE.with(|cell| {
            cell.set(HookState { hwnd, toggle_message, chord: WinKeyChord::new() });
        });

        let config = Config::load();
        let keyboard_hook: Option<HHOOK> = if config.use_win_key {
            // SAFETY: `keyboard_hook_proc` is a valid `WH_KEYBOARD_LL` procedure and this thread
            // pumps messages in `run_message_loop` until `TriggerWindow` drops.
            match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) } {
                Ok(hook) => Some(hook),
                Err(error) => {
                    eprintln!("failed to install the Windows-key hook, falling back to Ctrl+Alt+Space: {error}");
                    None
                }
            }
        } else {
            None
        };

        let taskbar_hook: Option<TaskbarHookGuard> = if config.use_taskbar_hook {
            match crate::taskbar::install_taskbar_hook(hwnd, toggle_message) {
                Ok(guard) => Some(guard),
                Err(error) => {
                    eprintln!("failed to install Explorer taskbar hook: {error}");
                    None
                }
            }
        } else {
            None
        };

        let window = Self { hwnd, toggle_message, taskbar_created_msg, keyboard_hook, taskbar_hook, _class: class };

        // SAFETY: `window.hwnd` is a live window owned by this thread.
        let hotkey = unsafe {
            RegisterHotKey(
                Some(window.hwnd),
                DEV_HOTKEY_ID,
                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                u32::from(VK_SPACE.0),
            )
        };

        // Not fatal: the hotkey is a development convenience and the toggle message still works.
        if let Err(error) = hotkey {
            eprintln!("Ctrl+Alt+Space is unavailable, likely taken by another application: {error}");
        }

        Ok(window)
    }

    fn reinstall_taskbar_hook(&mut self) {
        if Config::load().use_taskbar_hook {
            match crate::taskbar::install_taskbar_hook(self.hwnd, self.toggle_message) {
                Ok(guard) => {
                    self.taskbar_hook = Some(guard);
                }
                Err(error) => {
                    eprintln!("failed to re-install Explorer taskbar hook: {error}");
                }
            }
        }
    }

    fn update_taskbar_geometry(&self) {
        if let Some(ref hook) = self.taskbar_hook
            && let Ok(geom) = crate::taskbar::detect_taskbar_geometry()
        {
            hook.update_geometry(&geom);
        }
    }

    fn is_toggle_request(&self, message: &MSG) -> bool {
        let is_dev_hotkey: bool = message.message == WM_HOTKEY && message.wParam.0 == DEV_HOTKEY_ID as usize;

        is_dev_hotkey || message.message == self.toggle_message
    }
}

impl Drop for TriggerWindow {
    fn drop(&mut self) {
        // Drop taskbar hook first so it unhooks and unmaps before the trigger window is destroyed.
        self.taskbar_hook.take();

        // SAFETY: Drop runs on the creating thread because TriggerWindow never leaves it.
        // Failures are ignored: the hook or hotkey may never have been registered.
        unsafe {
            if let Some(hook) = self.keyboard_hook.take() {
                let _ = UnhookWindowsHookEx(hook);
            }
            let _ = UnregisterHotKey(Some(self.hwnd), DEV_HOTKEY_ID);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Low-level keyboard hook that detects a solo Windows-key tap (`VK_LWIN` / `VK_RWIN`), injects a
/// silent `VK_NONAME` key event so `explorer.exe` suppresses the built-in Windows 11 Start menu,
/// and posts the toggle message to `TriggerWindow`.
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        // SAFETY: when `code == HC_ACTION`, Windows passes a valid pointer to `KBDLLHOOKSTRUCT` in `lparam`.
        let kb: &KBDLLHOOKSTRUCT = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };

        // Ignore synthetic keystrokes (including our own `VK_NONAME` mask injection below).
        if !kb.flags.contains(LLKHF_INJECTED) {
            let msg: u32 = wparam.0 as u32;
            let is_key_event: bool = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP);

            if is_key_event && let Ok(vk) = u16::try_from(kb.vkCode) {
                let is_key_up: bool = matches!(msg, WM_KEYUP | WM_SYSKEYUP) || kb.flags.contains(LLKHF_UP);

                let should_toggle: Option<(HWND, u32)> = HOOK_STATE.with(|cell| {
                    let mut state = cell.get();
                    let triggered = state.chord.on_key_event(vk, is_key_up);
                    cell.set(state);
                    triggered.then_some((state.hwnd, state.toggle_message))
                });

                if let Some((hwnd, toggle_message)) = should_toggle {
                    suppress_native_start_menu();
                    // SAFETY: `hwnd` belongs to `TriggerWindow` on this thread and `toggle_message`
                    // is the registered `RustyStart.ToggleMenu` message ID.
                    let _ = unsafe { PostMessageW(Some(hwnd), toggle_message, WPARAM(0), LPARAM(0)) };
                }
            }
        }
    }

    // SAFETY: always forwards the event along the hook chain so `VK_LWIN`/`VK_RWIN` key-up is
    // delivered to the OS and the Windows modifier never gets stuck down.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Sends a silent `VK_NONAME` (`0xFC`) key-down and key-up before the `VK_LWIN` / `VK_RWIN`
/// release reaches `explorer.exe`, convincing Windows that a `Win+<Key>` chord occurred so it
/// does not open the built-in Start menu.
///
/// The menu window also calls this when the Windows key goes down while it has keyboard focus,
/// because the low-level hook does not see keys sent to the host's own window.
pub fn suppress_native_start_menu() {
    let inputs: [INPUT; 2] = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VK_NONAME, dwFlags: KEYBD_EVENT_FLAGS(0), ..Default::default() },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_NONAME, dwFlags: KEYEVENTF_KEYUP, ..Default::default() } },
        },
    ];

    // SAFETY: `inputs` is a valid slice of two `INPUT` structs with `cbsize` matching `size_of::<INPUT>()`.
    let _ = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

/// Registered window class, unregistered on drop.
struct WindowClass {
    name: HSTRING,
    instance: HINSTANCE,
}

impl WindowClass {
    fn register(name: &str) -> Result<Self> {
        // SAFETY: a null module name returns the handle of the running executable.
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("failed to get the module handle")?;
        let instance = HINSTANCE::from(module);
        let name = HSTRING::from(name);

        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_procedure),
            hInstance: instance,
            lpszClassName: PCWSTR(name.as_ptr()),
            ..Default::default()
        };

        // SAFETY: `class` is fully initialized and `name` outlives the call.
        if unsafe { RegisterClassExW(&raw const class) } == 0 {
            return Err(windows::core::Error::from_thread()).context("failed to register the window class");
        }

        Ok(Self { name, instance })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: the class was registered by `register`, and its only window is destroyed first.
        let _ = unsafe { UnregisterClassW(&self.name, Some(self.instance)) };
    }
}

fn create_hidden_window(class: &WindowClass) -> Result<HWND> {
    // SAFETY: the class is registered with this instance. The window is never shown.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            &class.name,
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(class.instance),
            None,
        )
    }
    .context("failed to create the trigger window")
}

fn register_toggle_message() -> Result<u32> {
    // SAFETY: the argument converts to a valid null-terminated wide string.
    let message_id: u32 = unsafe { RegisterWindowMessageW(&HSTRING::from(TOGGLE_MESSAGE_NAME)) };

    if message_id == 0 {
        return Err(windows::core::Error::from_thread()).context("failed to register the toggle message");
    }

    Ok(message_id)
}

fn register_taskbar_created_message() -> u32 {
    let name: HSTRING = HSTRING::from("TaskbarCreated");
    // SAFETY: RegisterWindowMessageW takes a valid null-terminated wide string.
    unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) }
}

/// Stateless window procedure. Trigger messages are handled in `run_message_loop`.
extern "system" fn window_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the arguments Windows passed to this procedure unchanged.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LWIN: u16 = VK_LWIN.0;
    const RWIN: u16 = VK_RWIN.0;
    const KEY_R: u16 = 0x52;
    const LCTRL: u16 = 0xA2;

    #[test]
    fn solo_windows_key_tap_triggers_on_release() {
        let mut chord = WinKeyChord::new();

        assert!(!chord.on_key_event(LWIN, false));
        assert!(chord.on_key_event(LWIN, true));
    }

    #[test]
    fn auto_repeated_solo_windows_key_still_triggers_on_release() {
        let mut chord = WinKeyChord::new();

        assert!(!chord.on_key_event(RWIN, false));
        assert!(!chord.on_key_event(RWIN, false));
        assert!(chord.on_key_event(RWIN, true));
    }

    #[test]
    fn win_plus_key_chord_does_not_trigger_menu() {
        let mut chord = WinKeyChord::new();

        assert!(!chord.on_key_event(LWIN, false));
        assert!(!chord.on_key_event(KEY_R, false));
        assert!(!chord.on_key_event(KEY_R, true));
        assert!(!chord.on_key_event(LWIN, true));
    }

    #[test]
    fn holding_modifier_before_pressing_win_does_not_trigger_menu() {
        let mut chord = WinKeyChord::new();

        assert!(!chord.on_key_event(LCTRL, false));
        assert!(!chord.on_key_event(LWIN, false));
        assert!(!chord.on_key_event(LWIN, true));
        assert!(!chord.on_key_event(LCTRL, true));

        // Subsequent solo tap after releasing Ctrl works normally.
        assert!(!chord.on_key_event(LWIN, false));
        assert!(chord.on_key_event(LWIN, true));
    }
}
