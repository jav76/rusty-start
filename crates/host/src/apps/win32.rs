//! Windows apps from the shell's Applications folder (`shell:AppsFolder`), which merges Start menu
//! shortcuts with packaged (Store) apps. Each item's parsing name is either an AppUserModelID or a
//! known-folder path, and both launch through `shell:AppsFolder\<parsing name>`. Icons come from
//! the same shell items, rendered by the shell's image factory.
//!
//! `watch` rebuilds the list on a background thread whenever the shell reports a change in the
//! Start Menu Programs folders or the Applications folder.

use std::collections::HashSet;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, GetDIBits, GetObjectW,
    HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, FOLDERID_CommonPrograms, FOLDERID_Programs, IEnumShellItems, IShellItem,
    IShellItemImageFactory, KF_FLAG_DEFAULT, SHCNE_CREATE, SHCNE_DELETE, SHCNE_MKDIR, SHCNE_RENAMEFOLDER,
    SHCNE_RENAMEITEM, SHCNE_RMDIR, SHCNE_UPDATEDIR, SHCNE_UPDATEITEM, SHCNRF_InterruptLevel, SHCNRF_NewDelivery,
    SHCNRF_RecursiveInterrupt, SHCNRF_SOURCE, SHCNRF_ShellLevel, SHChangeNotification_Lock,
    SHChangeNotification_Unlock, SHChangeNotifyDeregister, SHChangeNotifyEntry, SHChangeNotifyRegister,
    SHCreateItemFromParsingName, SHGetKnownFolderIDList, SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, SIIGBF_ICONONLY, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE, KillTimer, MSG,
    PostThreadMessageW, RegisterClassExW, SW_SHOWNORMAL, SetTimer, UnregisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_APP, WM_QUIT, WM_TIMER, WNDCLASSEXW,
};
use windows::core::{GUID, HSTRING, Interface, Owned, PCWSTR, w};

use super::{App, ICON_SIZE, IconPixels};

/// Number of items requested from the enumerator per call.
const ENUM_BATCH_SIZE: usize = 64;

/// Message the shell sends to the watcher window for each change. It arrives through the window
/// procedure, not the message queue.
const CHANGE_MESSAGE: u32 = WM_APP + 1;
const CHANGE_WINDOW_CLASS: PCWSTR = w!("RustyStart.AppWatchWindow");
const DEBOUNCE_TIMER_ID: usize = 1;
/// Installers touch many files in a burst, so the rebuild waits for the shell to go quiet.
const DEBOUNCE_MS: u32 = 1500;

/// Lists the Applications folder. Blocks for tens of milliseconds, so call it off the UI thread.
pub fn load() -> Result<Vec<App>> {
    let _com = ComApartment::initialize()?;

    // SAFETY: COM is initialized on this thread for as long as `_com` lives.
    let folder: IShellItem = unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) }
        .context("failed to open the Applications folder")?;
    // SAFETY: `folder` is a live shell item and BHID_EnumItems yields IEnumShellItems.
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems) }
        .context("failed to enumerate the Applications folder")?;

    let mut apps: Vec<App> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut batch: [Option<IShellItem>; ENUM_BATCH_SIZE] = std::array::from_fn(|_| None);

    loop {
        let mut fetched: u32 = 0;
        // SAFETY: `batch` and `fetched` are valid for writes for the duration of the call.
        unsafe { items.Next(&mut batch, Some(&raw mut fetched)) }
            .ok()
            .context("failed to read the Applications folder")?;

        for item in batch.iter_mut().take(fetched as usize).filter_map(Option::take) {
            if let Some(app) = read_app(&item)
                && seen_ids.insert(app.launch_id.clone())
            {
                apps.push(app);
            }
        }

        // S_FALSE with a short batch marks the end of the enumeration.
        if (fetched as usize) < ENUM_BATCH_SIZE {
            break;
        }
    }

    Ok(apps)
}

/// Renders the icon of each app, skipping apps the shell has no icon for. Takes a few
/// milliseconds per app with a warm shell icon cache and far longer with a cold one, so call it
/// off the UI thread.
pub fn load_icons(apps: &[App]) -> Result<Vec<(String, IconPixels)>> {
    let _com = ComApartment::initialize()?;
    let mut icons: Vec<(String, IconPixels)> = Vec::with_capacity(apps.len());
    let mut first_failure: Option<(String, anyhow::Error)> = None;
    let mut failures: usize = 0;

    for app in apps {
        match load_icon(&app.launch_id) {
            Ok(icon) => icons.push((app.launch_id.clone(), icon)),
            Err(error) => {
                failures += 1;
                first_failure.get_or_insert((app.name.clone(), error));
            }
        }
    }

    if let Some((name, error)) = first_failure {
        eprintln!("failed to load icons for {failures} apps, which show a placeholder (first: {name}: {error:#})");
    }

    Ok(icons)
}

fn load_icon(launch_id: &str) -> Result<IconPixels> {
    let path = HSTRING::from(format!("shell:AppsFolder\\{launch_id}"));
    // SAFETY: the caller initialized COM on this thread, and `path` is a valid wide string.
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(&path, None) }.context("failed to resolve the app")?;
    let factory: IShellItemImageFactory = item.cast().context("the app has no image factory")?;

    let size = SIZE { cx: ICON_SIZE as i32, cy: ICON_SIZE as i32 };
    // SAFETY: `factory` is a live shell image factory on a COM-initialized thread.
    let bitmap: HBITMAP = unsafe { factory.GetImage(size, SIIGBF_ICONONLY) }.context("failed to render the icon")?;
    // SAFETY: GetImage returns a new bitmap that the caller must delete.
    let bitmap: Owned<HBITMAP> = unsafe { Owned::new(bitmap) };

    read_bitmap(*bitmap)
}

/// Copies a bitmap that is not selected into any device context into premultiplied RGBA.
fn read_bitmap(bitmap: HBITMAP) -> Result<IconPixels> {
    let mut header = BITMAP::default();
    // SAFETY: `header` is a writable BITMAP, and the size passed matches it.
    let written: i32 =
        unsafe { GetObjectW(HGDIOBJ::from(bitmap), size_of::<BITMAP>() as i32, Some((&raw mut header).cast())) };
    if written == 0 || header.bmWidth <= 0 || header.bmHeight == 0 {
        bail!("failed to read the icon bitmap size");
    }

    let width: u32 = header.bmWidth as u32;
    let height: u32 = header.bmHeight.unsigned_abs();
    // A negative height requests top-down rows, the order Slint images use.
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut icon = IconPixels::new(width, height);
    let pixels: &mut [u8] = icon.make_mut_bytes();
    let memory_dc = MemoryDc::create()?;
    // SAFETY: `bitmap` is not selected into a DC, and `pixels` holds `height` rows of `width`
    // 32-bit pixels, which is what `info` describes.
    let rows: i32 = unsafe {
        GetDIBits(memory_dc.0, bitmap, 0, height, Some(pixels.as_mut_ptr().cast()), &raw mut info, DIB_RGB_COLORS)
    };
    if rows != height as i32 {
        bail!("failed to copy the icon pixels");
    }

    bgra_to_premultiplied_rgba(pixels);
    Ok(icon)
}

/// Converts the shell's 32-bit BGRA icon pixels, which carry straight alpha, to premultiplied
/// RGBA in place. Old icons have no alpha at all (every alpha zero) and are made opaque.
fn bgra_to_premultiplied_rgba(pixels: &mut [u8]) {
    let (pixels, _): (&mut [[u8; 4]], _) = pixels.as_chunks_mut::<4>();
    let has_alpha: bool = pixels.iter().any(|pixel| pixel[3] != 0);

    for pixel in pixels {
        pixel.swap(0, 2);
        if !has_alpha {
            pixel[3] = u8::MAX;
            continue;
        }

        let alpha: u16 = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

/// Memory device context for GetDIBits, deleted on drop.
struct MemoryDc(HDC);

impl MemoryDc {
    fn create() -> Result<Self> {
        // SAFETY: `None` creates a memory DC compatible with the screen.
        let dc: HDC = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            bail!("CreateCompatibleDC failed");
        }
        Ok(Self(dc))
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from CreateCompatibleDC and is deleted once.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// Launches `app` on a short-lived thread so a slow shell handler never stalls the UI.
pub fn launch(app: &App) {
    let target = HSTRING::from(format!("shell:AppsFolder\\{}", app.launch_id));
    let name: String = app.name.clone();

    let spawned = thread::Builder::new().name("rusty-start-launch".into()).spawn(move || {
        if let Err(error) = shell_open(&target) {
            eprintln!("failed to launch {name}: {error:#}");
        }
    });

    if let Err(error) = spawned {
        eprintln!("failed to spawn the launch thread: {error}");
    }
}

fn shell_open(target: &HSTRING) -> Result<()> {
    // ShellExecute may load shell extensions that require a single-threaded apartment.
    let _com = ComApartment::initialize()?;

    // SAFETY: every string argument is a valid null-terminated wide string or null.
    let result = unsafe { ShellExecuteW(None, w!("open"), target, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };

    // Values of 32 or below are error codes rather than instance handles.
    let code: usize = result.0 as usize;
    if code <= 32 {
        bail!("ShellExecuteW returned error code {code}");
    }

    Ok(())
}

fn read_app(item: &IShellItem) -> Option<App> {
    let name: String = display_name(item, SIGDN_NORMALDISPLAY).ok()?;
    let launch_id: String = display_name(item, SIGDN_PARENTRELATIVEPARSING).ok()?;

    if name.trim().is_empty() || launch_id.is_empty() {
        return None;
    }

    let keywords: Vec<String> = super::exe_keyword(&launch_id).into_iter().collect();
    Some(App { name, launch_id, keywords })
}

fn display_name(item: &IShellItem, form: SIGDN) -> Result<String> {
    // SAFETY: `item` is a live shell item on a COM-initialized thread.
    let raw = unsafe { item.GetDisplayName(form) }?;
    // SAFETY: GetDisplayName returned a valid null-terminated string.
    let text = unsafe { raw.to_string() };
    // SAFETY: the shell allocated the string with CoTaskMemAlloc and it is not used again.
    unsafe { CoTaskMemFree(Some(raw.0 as *const c_void)) };

    Ok(text?)
}

/// Single-threaded COM apartment for the current thread, left on drop.
struct ComApartment {
    // CoUninitialize must run on the initializing thread, so the guard must not be Send.
    _not_send: PhantomData<*const ()>,
}

impl ComApartment {
    fn initialize() -> Result<Self> {
        // SAFETY: paired with CoUninitialize in Drop, which runs on this thread.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
            .ok()
            .context("failed to initialize COM")?;

        Ok(Self { _not_send: PhantomData })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx in `initialize` on the same thread.
        unsafe { CoUninitialize() };
    }
}

/// Keeps the watcher thread running. Dropping it stops the thread and deregisters notifications.
pub struct AppWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Drop for AppWatcher {
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

/// Starts a thread that rebuilds the app list after installs and uninstalls. `on_change` runs on
/// that thread, not on the UI thread.
pub fn watch(on_change: impl Fn(Vec<App>) + Send + 'static) -> Result<AppWatcher> {
    let (ready_sender, ready_receiver) = mpsc::channel::<Result<u32>>();

    let thread: JoinHandle<()> = thread::Builder::new()
        .name("rusty-start-app-watch".into())
        .spawn(move || run_watch_thread(on_change, &ready_sender))
        .context("failed to spawn the app watcher thread")?;

    match ready_receiver.recv() {
        Ok(Ok(thread_id)) => Ok(AppWatcher { thread_id, thread: Some(thread) }),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            bail!("the app watcher thread exited during startup")
        }
    }
}

fn run_watch_thread(on_change: impl Fn(Vec<App>), ready: &mpsc::Sender<Result<u32>>) {
    let setup = ComApartment::initialize().and_then(|com| Ok((com, ChangeWindow::create()?)));
    // `_com` is declared first so COM is left only after the window and registrations are gone.
    let (_com, window) = match setup {
        Ok(setup) => setup,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    // SAFETY: GetCurrentThreadId has no preconditions.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is a valid out-parameter, and a `None` filter also receives WM_QUIT.
        if unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 <= 0 {
            break;
        }

        if message.message == WM_TIMER && message.wParam.0 == DEBOUNCE_TIMER_ID {
            // SAFETY: `change_window_procedure` set the timer on `window.hwnd`.
            let _ = unsafe { KillTimer(Some(window.hwnd), DEBOUNCE_TIMER_ID) };
            match load() {
                Ok(apps) => on_change(apps),
                Err(error) => eprintln!("failed to rebuild the app list: {error:#}"),
            }
            continue;
        }

        // SAFETY: `message` was filled in by the GetMessageW call above.
        unsafe { DispatchMessageW(&raw const message) };
    }
}

extern "system" fn change_window_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message != CHANGE_MESSAGE {
        // SAFETY: forwards the arguments Windows passed to this procedure unchanged.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }

    // With SHCNRF_NewDelivery the notification lives in shared memory that is freed by locking and
    // unlocking it. wParam is its handle and lParam the sending process ID.
    // SAFETY: both values come straight from a SHCNRF_NewDelivery notification.
    let lock = unsafe { SHChangeNotification_Lock(HANDLE(wparam.0 as *mut c_void), lparam.0 as u32, None, None) };
    if !lock.is_invalid() {
        // SAFETY: `lock` came from SHChangeNotification_Lock and is unlocked once.
        let _ = unsafe { SHChangeNotification_Unlock(lock) };
    }

    // Re-arming an existing timer restarts its countdown. WM_TIMER is posted to the queue, where
    // `run_watch_thread` picks it up.
    // SAFETY: `hwnd` is the watcher window, owned by this thread.
    unsafe { SetTimer(Some(hwnd), DEBOUNCE_TIMER_ID, DEBOUNCE_MS, None) };
    LRESULT(0)
}

/// Message-only window that receives the shell change notifications. Must stay on its thread.
struct ChangeWindow {
    hwnd: HWND,
    instance: HINSTANCE,
    registrations: Vec<u32>,
}

impl ChangeWindow {
    fn create() -> Result<Self> {
        // SAFETY: a null module name returns the handle of the running executable.
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("failed to get the module handle")?;
        let instance = HINSTANCE::from(module);
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(change_window_procedure),
            hInstance: instance,
            lpszClassName: CHANGE_WINDOW_CLASS,
            ..Default::default()
        };
        // SAFETY: `class` is fully initialized and its class name is a static wide string.
        if unsafe { RegisterClassExW(&raw const class) } == 0 {
            return Err(windows::core::Error::from_thread()).context("failed to register the app watcher class");
        }

        // SAFETY: the class is registered above, and HWND_MESSAGE makes the window message-only.
        let created = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CHANGE_WINDOW_CLASS,
                PCWSTR::null(),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )
        };
        let hwnd: HWND = match created {
            Ok(hwnd) => hwnd,
            Err(error) => {
                // SAFETY: the class was registered above and has no windows.
                let _ = unsafe { UnregisterClassW(CHANGE_WINDOW_CLASS, Some(instance)) };
                return Err(error).context("failed to create the app watcher window");
            }
        };

        let mut window = Self { hwnd, instance, registrations: Vec::new() };
        // Shortcut folders change on disk, so they need interrupt-level (file system) events too.
        let file_system = SHCNRF_ShellLevel | SHCNRF_InterruptLevel | SHCNRF_RecursiveInterrupt | SHCNRF_NewDelivery;
        window.register(&FOLDERID_Programs, file_system, true);
        window.register(&FOLDERID_CommonPrograms, file_system, true);
        // Packaged apps have no shortcut, so only the virtual Applications folder can report them.
        window.register(&FOLDERID_AppsFolder, SHCNRF_ShellLevel | SHCNRF_NewDelivery, false);

        if window.registrations.is_empty() {
            bail!("no shell change notifications could be registered");
        }

        Ok(window)
    }

    fn register(&mut self, folder: &GUID, sources: SHCNRF_SOURCE, recursive: bool) {
        // SAFETY: COM is initialized on this thread, and `folder` is a known folder ID.
        let Ok(pidl) = (unsafe { SHGetKnownFolderIDList(folder, KF_FLAG_DEFAULT.0 as u32, None) }) else {
            return;
        };

        let events = SHCNE_CREATE
            | SHCNE_DELETE
            | SHCNE_RENAMEITEM
            | SHCNE_RENAMEFOLDER
            | SHCNE_MKDIR
            | SHCNE_RMDIR
            | SHCNE_UPDATEDIR
            | SHCNE_UPDATEITEM;
        let entry = SHChangeNotifyEntry { pidl, fRecursive: recursive.into() };

        // SAFETY: `entry` is one valid entry, and the shell copies the ID list it points to.
        let id: u32 =
            unsafe { SHChangeNotifyRegister(self.hwnd, sources, events.0 as i32, CHANGE_MESSAGE, 1, &raw const entry) };
        // SAFETY: the shell allocated `pidl` with CoTaskMemAlloc, and the registration holds a copy.
        unsafe { CoTaskMemFree(Some(pidl as *const c_void)) };

        if id != 0 {
            self.registrations.push(id);
        }
    }
}

impl Drop for ChangeWindow {
    fn drop(&mut self) {
        // SAFETY: each ID came from SHChangeNotifyRegister, the window belongs to this thread, and the
        // class is unregistered only after its one window is destroyed.
        unsafe {
            for id in self.registrations.drain(..) {
                let _ = SHChangeNotifyDeregister(id);
            }
            let _ = DestroyWindow(self.hwnd);
            let _ = UnregisterClassW(CHANGE_WINDOW_CLASS, Some(self.instance));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_bgra_to_rgba_and_premultiplies() {
        let mut pixels: Vec<u8> = vec![255, 0, 200, 128, 9, 9, 9, 0];
        bgra_to_premultiplied_rgba(&mut pixels);

        assert_eq!(pixels, [100, 0, 128, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn keeps_opaque_pixels_unchanged() {
        let mut pixels: Vec<u8> = vec![10, 20, 30, 255];
        bgra_to_premultiplied_rgba(&mut pixels);

        assert_eq!(pixels, [30, 20, 10, 255]);
    }

    #[test]
    fn makes_icons_without_alpha_opaque() {
        let mut pixels: Vec<u8> = vec![1, 2, 3, 0, 4, 5, 6, 0];
        bgra_to_premultiplied_rgba(&mut pixels);

        assert_eq!(pixels, [3, 2, 1, 255, 6, 5, 4, 255]);
    }
}
