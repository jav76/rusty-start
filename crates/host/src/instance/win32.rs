use anyhow::{Context, Result};
use rusty_start_protocol::{MESSAGE_WINDOW_CLASS, QUIT_MESSAGE_NAME, SINGLE_INSTANCE_MUTEX_NAME, TOGGLE_MESSAGE_NAME};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, PostMessageW, RegisterWindowMessageW,
};
use windows::core::{HSTRING, PCWSTR};

use super::HostRequest;

/// Holds the single-instance mutex open for as long as the host runs.
pub struct InstanceGuard {
    mutex: HANDLE,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: `mutex` came from CreateMutexW in `acquire` and is closed once.
        let _ = unsafe { CloseHandle(self.mutex) };
    }
}

/// Returns `None` when another host already runs in this session.
pub fn acquire() -> Result<Option<InstanceGuard>> {
    // SAFETY: the name converts to a valid null-terminated wide string.
    let mutex: HANDLE = unsafe { CreateMutexW(None, false, &HSTRING::from(SINGLE_INSTANCE_MUTEX_NAME)) }
        .context("failed to create the single-instance mutex")?;

    // CreateMutexW opens the existing mutex and reports ERROR_ALREADY_EXISTS through the last error.
    // SAFETY: GetLastError has no preconditions and runs right after CreateMutexW on this thread.
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return Ok(Some(InstanceGuard { mutex }));
    }

    // SAFETY: `mutex` is the handle opened above and is not used again.
    let _ = unsafe { CloseHandle(mutex) };
    Ok(None)
}

/// Posts `request` to the running host's trigger window. Returns `false` when no host runs.
pub fn send(request: HostRequest) -> Result<bool> {
    // SAFETY: the class name converts to a valid null-terminated wide string.
    let Ok(hwnd) = (unsafe { FindWindowW(&HSTRING::from(MESSAGE_WINDOW_CLASS), PCWSTR::null()) }) else {
        return Ok(false);
    };

    let message_name: &str = match request {
        HostRequest::Toggle => TOGGLE_MESSAGE_NAME,
        HostRequest::Quit => QUIT_MESSAGE_NAME,
    };
    // SAFETY: the message name converts to a valid null-terminated wide string.
    let message: u32 = unsafe { RegisterWindowMessageW(&HSTRING::from(message_name)) };
    if message == 0 {
        return Err(windows::core::Error::from_thread()).context("failed to register the host message");
    }

    if request == HostRequest::Toggle {
        allow_foreground_for_window_owner(hwnd);
    }

    // SAFETY: `hwnd` is the running host's trigger window and the message is registered.
    unsafe { PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0)) }
        .context("failed to post the request to the running Rusty Start")?;
    Ok(true)
}

/// A launch from Explorer or a terminal owns the foreground, so it can pass that right on.
fn allow_foreground_for_window_owner(hwnd: HWND) {
    let mut pid: u32 = 0;
    // SAFETY: `pid` is a valid out-parameter for the duration of the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
    if pid != 0 {
        // SAFETY: AllowSetForegroundWindow has no memory-safety preconditions.
        let _ = unsafe { AllowSetForegroundWindow(pid) };
    }
}
