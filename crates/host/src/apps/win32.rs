//! Windows apps from the shell's Applications folder (`shell:AppsFolder`), which merges Start menu
//! shortcuts with packaged (Store) apps. Each item's parsing name is either an AppUserModelID or a
//! known-folder path, and both launch through `shell:AppsFolder\<parsing name>`.

use std::collections::HashSet;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::thread;

use anyhow::{Context, Result, bail};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT, SHGetKnownFolderItem, SIGDN,
    SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use super::App;

/// Number of items requested from the enumerator per call.
const ENUM_BATCH_SIZE: usize = 64;

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
        unsafe { items.Next(&mut batch, Some(&mut fetched)) }.ok().context("failed to read the Applications folder")?;

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

    Some(App { name, launch_id })
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
