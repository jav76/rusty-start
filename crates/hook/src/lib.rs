//! In-process Explorer taskbar hook DLL.
//!
//! Injected into `explorer.exe` on the taskbar thread via `SetWindowsHookExW(WH_GETMESSAGE)`.
//! Intercepts primary mouse clicks on the Start button, suppresses them (`WM_NULL`) so Explorer's
//! default XAML Start menu never opens, and posts `RustyStart.ToggleMenu` to the host trigger window.

#![cfg_attr(windows, allow(non_snake_case))]

#[cfg(windows)]
mod win32 {
    use std::sync::Mutex;

    use rusty_start_protocol::{HOOK_SHARED_MEMORY_NAME, HookSharedState};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Memory::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile};
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, GetWindowThreadProcessId, PostMessageW};
    use windows::core::HSTRING;

    struct SharedMapping {
        handle: HANDLE,
        ptr: *const HookSharedState,
    }

    // SAFETY: HookSharedState is Copy, plain-old-data, and read-only from the hook DLL.
    unsafe impl Send for SharedMapping {}
    // SAFETY: HookSharedState is Copy, plain-old-data, and read-only from the hook DLL.
    unsafe impl Sync for SharedMapping {}

    impl Drop for SharedMapping {
        fn drop(&mut self) {
            if !self.ptr.is_null() {
                // SAFETY: The memory was previously mapped by MapViewOfFile.
                unsafe {
                    let _ = UnmapViewOfFile(windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: self.ptr as *mut std::ffi::c_void,
                    });
                }
            }
            if !self.handle.is_invalid() {
                // SAFETY: The handle was successfully returned by OpenFileMappingW.
                unsafe {
                    let _ = CloseHandle(self.handle);
                }
            }
        }
    }

    static MAPPING: Mutex<Option<SharedMapping>> = Mutex::new(None);

    fn get_shared_state() -> Option<HookSharedState> {
        let mut guard = MAPPING.lock().ok()?;
        if guard.is_none() {
            let wide_name = HSTRING::from(HOOK_SHARED_MEMORY_NAME);
            // SAFETY: Opening read-only file mapping by name with standard Win32 flags.
            let handle_res =
                unsafe { OpenFileMappingW(FILE_MAP_READ.0, false, windows::core::PCWSTR(wide_name.as_ptr())) };
            if let Ok(handle) = handle_res {
                // SAFETY: Mapping the read-only file view with the exact size of HookSharedState.
                let view =
                    unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, std::mem::size_of::<HookSharedState>()) };
                if !view.Value.is_null() {
                    *guard = Some(SharedMapping { handle, ptr: view.Value as *const HookSharedState });
                } else {
                    // SAFETY: Mapping failed; clean up the open handle.
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                }
            }
        }

        if let Some(ref m) = *guard
            && !m.ptr.is_null()
        {
            // SAFETY: Pointer is valid, aligned, points to HookSharedState, and read atomically/by-value.
            return Some(unsafe { std::ptr::read_volatile(m.ptr) });
        }
        None
    }

    pub fn detach() {
        if let Ok(mut guard) = MAPPING.lock() {
            *guard = None;
        }
    }

    pub fn should_intercept(x: i32, y: i32) -> bool {
        if let Some(state) = get_shared_state()
            && state.enabled
        {
            if state.primary_start_rect.contains(x, y) {
                return true;
            }
            if state.has_secondary && state.secondary_start_rect.contains(x, y) {
                return true;
            }
        }
        false
    }

    pub fn dispatch_toggle() {
        if let Some(state) = get_shared_state()
            && state.trigger_hwnd != 0
            && state.toggle_msg_id != 0
        {
            let trigger_hwnd = HWND(state.trigger_hwnd as *mut std::ffi::c_void);
            let mut host_pid: u32 = 0;
            // SAFETY: `host_pid` is a valid out-parameter. A stale handle only yields a zero pid.
            unsafe { GetWindowThreadProcessId(trigger_hwnd, Some(&raw mut host_pid)) };

            // Explorer owns the foreground after a taskbar click, so it can hand that right to the
            // host. Without it the host's `SetForegroundWindow` fails and the menu opens unfocused.
            if host_pid != 0 {
                // SAFETY: AllowSetForegroundWindow has no memory-safety preconditions.
                let _ = unsafe { AllowSetForegroundWindow(host_pid) };
            }

            // SAFETY: Posting the registered toggle message to the host window asynchronously.
            unsafe {
                let _ = PostMessageW(Some(trigger_hwnd), state.toggle_msg_id, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Standard DLL entry point called by the OS loader during process and thread attachment.
///
/// # Safety
///
/// Called by the Windows loader with valid module instance handle and process attachment flags.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    hinst_dll: windows::Win32::Foundation::HINSTANCE,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> windows::core::BOOL {
    const DLL_PROCESS_ATTACH: u32 = 1;
    const DLL_PROCESS_DETACH: u32 = 0;

    if reason == DLL_PROCESS_ATTACH {
        // SAFETY: hinst_dll is the module instance passed by the OS loader during process attachment.
        unsafe {
            let _ = windows::Win32::System::LibraryLoader::DisableThreadLibraryCalls(hinst_dll.into());
        }
    } else if reason == DLL_PROCESS_DETACH {
        win32::detach();
    }
    windows::core::BOOL::from(true)
}

/// Exported hook procedure installed via `SetWindowsHookExW(WH_GETMESSAGE, ...)`.
///
/// Wraps all logic in `catch_unwind` so panics never unwind across the FFI boundary into Explorer.
///
/// # Safety
///
/// When `code >= 0`, `lparam` must point to a valid Win32 `MSG` structure supplied by `GetMessageW` or `PeekMessageW`.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn rusty_start_hook_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if code >= 0 && lparam.0 != 0 {
            // SAFETY: In WH_GETMESSAGE when code >= 0, lparam points to a valid MSG structure.
            let msg = unsafe { &mut *(lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MSG) };
            let is_down = msg.message == windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN;
            let is_up = msg.message == windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONUP;

            if (is_down || is_up) && win32::should_intercept(msg.pt.x, msg.pt.y) {
                // Suppress the click before Explorer's window procedure receives it.
                msg.message = windows::Win32::UI::WindowsAndMessaging::WM_NULL;
                if is_up {
                    win32::dispatch_toggle();
                }
            }
        }
    }));

    // SAFETY: Forwarding hook message to the next hook in the chain as required by Win32 hooks.
    unsafe { windows::Win32::UI::WindowsAndMessaging::CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(not(windows))]
#[unsafe(no_mangle)]
pub extern "system" fn rusty_start_hook_proc(_code: i32, _wparam: usize, _lparam: isize) -> isize {
    0
}
