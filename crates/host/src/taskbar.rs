//! Taskbar Start button geometry detection and hook injection.
//!
//! Locates the Windows 11 Taskbar Start button bounding rectangles across primary and secondary
//! monitors, maintains the shared memory state (`HookSharedState`), and manages the lifecycle of the
//! `WH_GETMESSAGE` hook injected into `explorer.exe`.

pub mod win32 {
    use anyhow::{Context, Result, bail};
    use rusty_start_protocol::{HOOK_DLL_NAME, HOOK_SHARED_MEMORY_NAME, HookSharedState, Rect};
    use std::env;
    use std::path::PathBuf;
    use std::ptr;
    use windows::Win32::Foundation::{CloseHandle, FreeLibrary, HANDLE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE,
        UnmapViewOfFile,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_AutomationIdPropertyId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, GetWindowRect, GetWindowThreadProcessId, HHOOK, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_GETMESSAGE,
    };
    use windows::core::{BSTR, HSTRING, PCWSTR, s, w};

    /// Geometry and thread information for detected taskbars.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TaskbarGeometry {
        pub primary_tray_hwnd: HWND,
        pub primary_tray_tid: u32,
        pub primary_start_rect: Rect,
        pub secondary_start_rect: Rect,
        pub has_secondary: bool,
    }

    /// Queries the Start button rectangle for a given taskbar HWND.
    ///
    /// Attempts UI Automation first (which accurately tracks centered or left-aligned XAML buttons
    /// in Windows 11), with fallback to Win32 child window inspection if UIA is unavailable.
    pub fn query_start_button_rect(tray_hwnd: HWND, automation: Option<&IUIAutomation>) -> Option<Rect> {
        if let Some(uia) = automation {
            // SAFETY: Calling UIA on an existing taskbar HWND from our host thread.
            if let Ok(elem) = unsafe { uia.ElementFromHandle(tray_hwnd) } {
                let var = VARIANT::from(BSTR::from("StartButton"));
                // SAFETY: Creating property condition for AutomationId == "StartButton".
                if let Ok(cond) = unsafe { uia.CreatePropertyCondition(UIA_AutomationIdPropertyId, &var) } {
                    // SAFETY: Searching descendants of the taskbar element.
                    if let Ok(btn) = unsafe { elem.FindFirst(TreeScope_Descendants, &cond) } {
                        // SAFETY: Reading bounding rectangle of the discovered Start button.
                        if let Ok(r) = unsafe { btn.CurrentBoundingRectangle() } {
                            let rect = Rect::new(r.left, r.top, r.right, r.bottom);
                            if rect.is_valid() {
                                return Some(rect);
                            }
                        }
                    }
                }
            }
        }

        // Fallback: check for legacy "Start" child window inside the taskbar.
        // SAFETY: Finding child window with class "Start" within tray_hwnd.
        let child = unsafe { FindWindowExW(Some(tray_hwnd), None, w!("Start"), None) }.ok()?;
        let mut r = windows::Win32::Foundation::RECT::default();
        // SAFETY: Querying window rectangle for the legacy Start button HWND.
        let ok = unsafe { GetWindowRect(child, &raw mut r) };
        if ok.is_ok() {
            let rect = Rect::new(r.left, r.top, r.right, r.bottom);
            if rect.is_valid() {
                return Some(rect);
            }
        }

        None
    }

    /// Discovers all taskbars on screen and queries their Start button rectangles.
    pub fn detect_taskbar_geometry() -> Result<TaskbarGeometry> {
        // SAFETY: Locating primary taskbar window by class name.
        let primary_hwnd =
            unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.context("failed to locate Shell_TrayWnd")?;

        let mut primary_pid = 0u32;
        // SAFETY: Obtaining thread ID and process ID of primary taskbar window.
        let primary_tid = unsafe { GetWindowThreadProcessId(primary_hwnd, Some(&raw mut primary_pid)) };
        if primary_tid == 0 {
            bail!("failed to obtain thread ID for Shell_TrayWnd");
        }

        // Initialize COM UIA if possible for high-accuracy bounding rectangle queries.
        // SAFETY: Instantiating CUIAutomation on the calling thread.
        let automation: Option<IUIAutomation> =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };

        let primary_rect = query_start_button_rect(primary_hwnd, automation.as_ref()).unwrap_or_else(|| {
            // If undetectable, estimate default left-corner Start button based on taskbar rect.
            let mut tray_r = windows::Win32::Foundation::RECT::default();
            // SAFETY: Querying primary taskbar bounds.
            let _ = unsafe { GetWindowRect(primary_hwnd, &raw mut tray_r) };
            Rect::new(tray_r.left, tray_r.top, tray_r.left + 50, tray_r.bottom)
        });

        // Search for secondary taskbar window (multi-monitor setups).
        let mut secondary_rect = Rect::default();
        let mut has_secondary = false;

        // SAFETY: Finding secondary taskbars by class name.
        if let Ok(sec_hwnd) = unsafe { FindWindowW(w!("Shell_SecondaryTrayWnd"), None) }
            && let Some(r) = query_start_button_rect(sec_hwnd, automation.as_ref())
        {
            secondary_rect = r;
            has_secondary = true;
        }

        Ok(TaskbarGeometry {
            primary_tray_hwnd: primary_hwnd,
            primary_tray_tid: primary_tid,
            primary_start_rect: primary_rect,
            secondary_start_rect: secondary_rect,
            has_secondary,
        })
    }

    /// Resolves the absolute path to `rusty_start_hook.dll`.
    pub fn find_hook_dll_path() -> Result<PathBuf> {
        let exe_path = env::current_exe().context("failed to get current executable path")?;
        let exe_dir = exe_path.parent().context("executable has no parent directory")?;

        // 1. Same directory as current executable (standard release / target directory).
        let candidate = exe_dir.join(HOOK_DLL_NAME);
        if candidate.exists() {
            return Ok(candidate);
        }

        // 2. Parent directory (e.g. deps folder during test runs).
        if let Some(parent) = exe_dir.parent() {
            let candidate = parent.join(HOOK_DLL_NAME);
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        bail!("could not find {HOOK_DLL_NAME} in {}", exe_dir.display())
    }

    /// RAII guard managing the shared memory and injected `WH_GETMESSAGE` taskbar hook.
    pub struct TaskbarHookGuard {
        mapping_handle: HANDLE,
        mapped_ptr: *mut HookSharedState,
        hook_handle: HHOOK,
        dll_module: HMODULE,
    }

    // SAFETY: The taskbar hook guard is safe to transfer between threads during setup.
    unsafe impl Send for TaskbarHookGuard {}

    impl TaskbarHookGuard {
        /// Updates the Start button bounding rectangles in shared memory without re-injecting.
        pub fn update_geometry(&self, geom: &TaskbarGeometry) {
            if !self.mapped_ptr.is_null() {
                // SAFETY: Updating shared memory atomically/volatilely with fresh geometry.
                unsafe {
                    let mut state = ptr::read_volatile(self.mapped_ptr);
                    state.primary_start_rect = geom.primary_start_rect;
                    state.secondary_start_rect = geom.secondary_start_rect;
                    state.has_secondary = geom.has_secondary;
                    ptr::write_volatile(self.mapped_ptr, state);
                }
            }
        }
    }

    impl Drop for TaskbarHookGuard {
        fn drop(&mut self) {
            // Disable interception first so in-flight clicks are not swallowed during teardown.
            if !self.mapped_ptr.is_null() {
                // SAFETY: Disabling hook state before unhooking.
                unsafe {
                    let mut state = ptr::read_volatile(self.mapped_ptr);
                    state.enabled = false;
                    ptr::write_volatile(self.mapped_ptr, state);
                }
            }

            if !self.hook_handle.is_invalid() {
                // SAFETY: Unhooking the WH_GETMESSAGE hook from Explorer's taskbar thread.
                unsafe {
                    let _ = UnhookWindowsHookEx(self.hook_handle);
                }
            }

            if !self.dll_module.is_invalid() {
                // SAFETY: Releasing our host reference to the hook DLL module.
                unsafe {
                    let _ = FreeLibrary(self.dll_module);
                }
            }

            if !self.mapped_ptr.is_null() {
                // SAFETY: Unmapping the shared memory view.
                unsafe {
                    let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: self.mapped_ptr.cast::<std::ffi::c_void>(),
                    });
                }
            }

            if !self.mapping_handle.is_invalid() {
                // SAFETY: Closing the named file mapping handle.
                unsafe {
                    let _ = CloseHandle(self.mapping_handle);
                }
            }
        }
    }

    /// Installs the taskbar Start-button hook into `explorer.exe`.
    pub fn install_taskbar_hook(trigger_hwnd: HWND, toggle_msg_id: u32) -> Result<TaskbarHookGuard> {
        let geom = detect_taskbar_geometry()?;
        let dll_path = find_hook_dll_path()?;

        // 1. Create or open the named shared memory mapping.
        let wide_name = HSTRING::from(HOOK_SHARED_MEMORY_NAME);
        let mapping_size = std::mem::size_of::<HookSharedState>() as u32;

        // SAFETY: Creating named file mapping backed by page file for IPC.
        let mapping_handle = unsafe {
            CreateFileMappingW(
                HANDLE(ptr::null_mut()),
                None,
                PAGE_READWRITE,
                0,
                mapping_size,
                PCWSTR(wide_name.as_ptr()),
            )?
        };

        // SAFETY: Mapping read-write view of the shared state for initialization.
        let view =
            unsafe { MapViewOfFile(mapping_handle, FILE_MAP_ALL_ACCESS, 0, 0, std::mem::size_of::<HookSharedState>()) };
        if view.Value.is_null() {
            // SAFETY: Cleanup handle on view mapping failure.
            unsafe {
                let _ = CloseHandle(mapping_handle);
            }
            bail!("failed to map view of hook shared memory");
        }
        let mapped_ptr = view.Value.cast::<HookSharedState>();

        // Populate initial state.
        let initial_state = HookSharedState {
            primary_start_rect: geom.primary_start_rect,
            secondary_start_rect: geom.secondary_start_rect,
            has_secondary: geom.has_secondary,
            enabled: true,
            trigger_hwnd: trigger_hwnd.0 as usize,
            toggle_msg_id,
        };
        // SAFETY: Writing initial HookSharedState to mapped shared memory.
        unsafe {
            ptr::write_volatile(mapped_ptr, initial_state);
        }

        // 2. Load the hook DLL in the host process to resolve the hook procedure.
        let dll_path_wide = HSTRING::from(dll_path.as_os_str());
        // SAFETY: Loading hook DLL into host process memory space.
        let dll_module = unsafe { LoadLibraryW(PCWSTR(dll_path_wide.as_ptr())) }
            .context("failed to load rusty_start_hook.dll into host")?;

        // SAFETY: Resolving exported hook procedure address by name.
        let proc_addr = unsafe { GetProcAddress(dll_module, s!("rusty_start_hook_proc")) }
            .context("failed to find rusty_start_hook_proc in DLL")?;

        // SAFETY: Converting raw function pointer to HookProc signature expected by SetWindowsHookExW.
        let hook_proc: unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT =
            unsafe { std::mem::transmute(proc_addr) };

        // 3. Install the WH_GETMESSAGE hook on Explorer's taskbar thread.
        // SAFETY: Installing hook on the specific taskbar thread ID (geom.primary_tray_tid).
        // Windows injects dll_module into explorer.exe when events arrive.
        let hook_handle = unsafe {
            SetWindowsHookExW(WH_GETMESSAGE, Some(hook_proc), Some(dll_module.into()), geom.primary_tray_tid)
        }
        .context("failed to install WH_GETMESSAGE hook on Explorer taskbar thread")?;

        Ok(TaskbarHookGuard { mapping_handle, mapped_ptr, hook_handle, dll_module })
    }
}

pub use win32::*;
