//! Windows trigger: a hidden window on a dedicated thread.
//!
//! The window is top-level rather than message-only (`HWND_MESSAGE`) because only top-level
//! windows receive the `TaskbarCreated` broadcast, which the host will need to re-inject the
//! hook after explorer restarts.
//!
//! Trigger messages are always posted, so the message loop handles them directly and the window
//! procedure stays stateless.

use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};
use rusty_start_protocol::{MESSAGE_WINDOW_CLASS, TOGGLE_MESSAGE_NAME};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, PostThreadMessageW,
    RegisterClassExW, RegisterWindowMessageW, UnregisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_HOTKEY, WM_QUIT,
    WNDCLASSEXW,
};
use windows::core::{HSTRING, PCWSTR};

/// Identifies the development hotkey, Ctrl+Alt+Space, which stands in for the explorer hook.
const DEV_HOTKEY_ID: i32 = 1;

/// Keeps the trigger thread running. Dropping it stops the thread and releases the hotkey.
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
        .spawn(move || run_trigger_thread(on_toggle, ready_sender))
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

fn run_trigger_thread(on_toggle: impl Fn(), ready: mpsc::Sender<Result<u32>>) {
    let window: TriggerWindow = match TriggerWindow::create() {
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

    run_message_loop(&window, on_toggle);
}

fn run_message_loop(window: &TriggerWindow, on_toggle: impl Fn()) {
    let mut message = MSG::default();

    loop {
        // SAFETY: `message` is a valid out-parameter. A `None` window filter receives every
        // message for this thread, including the WM_QUIT posted by TriggerGuard.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };

        // 0 means WM_QUIT was received and -1 means the call failed; both end the loop.
        if result.0 <= 0 {
            break;
        }

        if window.is_toggle_request(&message) {
            on_toggle();
            continue;
        }

        // SAFETY: `message` was filled in by the GetMessageW call above.
        unsafe { DispatchMessageW(&message) };
    }
}

/// Hidden window that owns the hotkey registration. Must stay on the thread that created it.
struct TriggerWindow {
    hwnd: HWND,
    toggle_message: u32,
    // Declared last so the class is unregistered only after Drop has destroyed the window.
    _class: WindowClass,
}

impl TriggerWindow {
    fn create() -> Result<Self> {
        let toggle_message: u32 = register_toggle_message()?;
        let class: WindowClass = WindowClass::register(MESSAGE_WINDOW_CLASS)?;
        let hwnd: HWND = create_hidden_window(&class)?;
        let window = Self { hwnd, toggle_message, _class: class };

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

    fn is_toggle_request(&self, message: &MSG) -> bool {
        let is_dev_hotkey: bool = message.message == WM_HOTKEY && message.wParam.0 == DEV_HOTKEY_ID as usize;

        is_dev_hotkey || message.message == self.toggle_message
    }
}

impl Drop for TriggerWindow {
    fn drop(&mut self) {
        // SAFETY: Drop runs on the creating thread because TriggerWindow never leaves it.
        // Failures are ignored: the hotkey may never have been registered.
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), DEV_HOTKEY_ID);
            let _ = DestroyWindow(self.hwnd);
        }
    }
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
        if unsafe { RegisterClassExW(&class) } == 0 {
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

/// Stateless window procedure. Trigger messages are handled in `run_message_loop`.
extern "system" fn window_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the arguments Windows passed to this procedure unchanged.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
