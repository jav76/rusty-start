//! Linux apps from freedesktop Desktop Entry files in `$XDG_DATA_HOME/applications` and
//! `$XDG_DATA_DIRS/*/applications`, which is what desktop launchers list. Launch IDs are desktop
//! file IDs (`org.kde.konsole.desktop`). Icons come from the current icon theme.
//!
//! Launched apps run in their own transient systemd user scope, so they keep running when the host
//! restarts or crashes. `watch` rebuilds the list when an applications directory changes.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use resvg::{tiny_skia, usvg};
use rustix::event::{EventfdFlags, PollFd, PollFlags, Timespec};
use rustix::fs::inotify;
use zbus::zvariant::Value;

use super::desktop_entry::{self, DesktopEntry};
use super::exec::{self, ExecContext};
use super::icon_theme::{self, IconLookup};
use super::{App, ICON_SIZE, IconPixels, raster};
use crate::dbus;

/// Installers write many files in a burst, so the rebuild waits for the directories to go quiet.
const DEBOUNCE: Duration = Duration::from_millis(1500);
/// Launcher segment of scope names, following systemd's `app-<launcher>-<app ID>-<random>.scope`.
const SCOPE_LAUNCHER: &str = "RustyStart";

/// Desktop file of each listed app, by launch ID. `load` replaces it; icons and launches read it.
static ENTRY_FILES: LazyLock<Mutex<HashMap<String, EntryFile>>> = LazyLock::new(Mutex::default);

#[derive(Clone, Debug)]
struct EntryFile {
    path: PathBuf,
    icon: Option<String>,
}

/// Lists the visible desktop entries. Reads a few hundred small files, so call it off the UI
/// thread.
#[expect(clippy::unnecessary_wraps, reason = "signature matches the fallible win32 backend")]
pub fn load() -> Result<Vec<App>> {
    let locales: Vec<String> = desktop_entry::locale_variants(&current_locale());
    let desktops: Vec<String> = current_desktops();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut apps: Vec<App> = Vec::new();
    let mut entry_files: HashMap<String, EntryFile> = HashMap::new();

    for applications_dir in applications_dirs() {
        for path in desktop_files(&applications_dir) {
            let Some(id) = desktop_entry::desktop_file_id(&applications_dir, &path) else {
                continue;
            };
            // An entry earlier in the search path masks later ones with the same ID, even when it
            // is hidden: that is how users remove system entries.
            if !seen_ids.insert(id.clone()) {
                continue;
            }
            let Some(entry) =
                fs::read_to_string(&path).ok().and_then(|contents| DesktopEntry::parse(&contents, &locales))
            else {
                continue;
            };
            if !entry.is_visible(&desktops, program_exists) {
                continue;
            }

            entry_files.insert(id.clone(), EntryFile { path, icon: entry.icon });
            apps.push(App { name: entry.name, launch_id: id });
        }
    }

    *ENTRY_FILES.lock().unwrap_or_else(PoisonError::into_inner) = entry_files;
    Ok(apps)
}

/// Renders the theme icon of each app, skipping apps without one. Call it off the UI thread.
#[expect(clippy::unnecessary_wraps, reason = "signature matches the fallible win32 backend")]
pub fn load_icons(apps: &[App]) -> Result<Vec<(String, IconPixels)>> {
    let icon_names: Vec<(String, String)> = {
        let entry_files = ENTRY_FILES.lock().unwrap_or_else(PoisonError::into_inner);
        apps.iter()
            .filter_map(|app| Some((app.launch_id.clone(), entry_files.get(&app.launch_id)?.icon.clone()?)))
            .collect()
    };

    let mut lookup =
        IconLookup::new(&current_icon_theme(), icon_base_dirs(), vec![PathBuf::from("/usr/share/pixmaps")]);
    let mut icons: Vec<(String, IconPixels)> = Vec::with_capacity(icon_names.len());
    let mut first_failure: Option<(String, anyhow::Error)> = None;
    let mut failures: usize = 0;

    for (launch_id, icon) in icon_names {
        let rendered: Result<IconPixels> = lookup
            .find(&icon, ICON_SIZE)
            .with_context(|| format!("no icon named {icon}"))
            .and_then(|path| render_icon(&path));
        match rendered {
            Ok(pixels) => icons.push((launch_id, pixels)),
            Err(error) => {
                failures += 1;
                first_failure.get_or_insert((launch_id, error));
            }
        }
    }

    if let Some((launch_id, error)) = first_failure {
        eprintln!("failed to load icons for {failures} apps, which show a placeholder (first: {launch_id}: {error:#})");
    }
    Ok(icons)
}

fn render_icon(path: &Path) -> Result<IconPixels> {
    let data: Vec<u8> = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let rgba: Vec<u8> = match path.extension().and_then(|extension| extension.to_str()) {
        Some("svg") => render_svg(&data)?,
        _ => decode_png(&data)?,
    };
    Ok(IconPixels::clone_from_slice(&rgba, ICON_SIZE, ICON_SIZE))
}

fn render_svg(data: &[u8]) -> Result<Vec<u8>> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).context("failed to parse the SVG icon")?;
    let mut pixmap = tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE).context("icon size is not zero")?;

    let size: tiny_skia::Size = tree.size();
    let scale: f32 = ICON_SIZE as f32 / size.width().max(size.height());
    let offset_x: f32 = (ICON_SIZE as f32 - size.width() * scale) / 2.0;
    let offset_y: f32 = (ICON_SIZE as f32 - size.height() * scale) / 2.0;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(offset_x, offset_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia already stores premultiplied RGBA.
    Ok(pixmap.take())
}

fn decode_png(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().context("failed to read the PNG header")?;
    let mut buffer: Vec<u8> = vec![0; reader.output_buffer_size().context("the PNG is too large")?];
    let frame = reader.next_frame(&mut buffer).context("failed to decode the PNG")?;
    buffer.truncate(frame.buffer_size());

    let mut rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::GrayscaleAlpha => {
            buffer.as_chunks::<2>().0.iter().flat_map(|&[gray, alpha]| [gray, gray, gray, alpha]).collect()
        }
        other => bail!("unexpected PNG color type {other:?}"),
    };
    raster::premultiply(&mut rgba);
    Ok(raster::fit_square(&rgba, frame.width, frame.height, ICON_SIZE))
}

/// Launches `app` on a background thread, which then waits for it so it never stays a zombie.
pub fn launch(app: &App) {
    let launch_id: String = app.launch_id.clone();
    let entry_file: Option<EntryFile> =
        ENTRY_FILES.lock().unwrap_or_else(PoisonError::into_inner).get(&launch_id).cloned();
    let Some(entry_file) = entry_file else {
        eprintln!("failed to launch {}: its desktop entry is no longer listed", app.name);
        return;
    };

    let spawned = thread::Builder::new().name("rusty-start-launch".into()).spawn(move || {
        if let Err(error) = launch_entry(&launch_id, &entry_file.path) {
            eprintln!("failed to launch {launch_id}: {error:#}");
        }
    });
    if let Err(error) = spawned {
        eprintln!("failed to spawn the launch thread: {error}");
    }
}

fn launch_entry(launch_id: &str, path: &Path) -> Result<()> {
    let contents: String = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let entry: DesktopEntry = DesktopEntry::parse(&contents, &[]).context("the desktop entry is no longer valid")?;
    let app_id: &str = launch_id.strip_suffix(".desktop").unwrap_or(launch_id);

    if entry.dbus_activatable {
        match activate_over_dbus(app_id) {
            Ok(()) => return Ok(()),
            Err(error) if entry.exec.is_some() => {
                eprintln!("D-Bus activation of {app_id} failed, running Exec: {error:#}");
            }
            Err(error) => return Err(error),
        }
    }

    let exec: &str = entry.exec.as_deref().context("the desktop entry has no Exec")?;
    let desktop_file: String = path.to_string_lossy().into_owned();
    let mut argv: Vec<String> = exec::expand(
        exec,
        &ExecContext { name: &entry.name, icon: entry.icon.as_deref(), desktop_file: &desktop_file },
    )?;
    if entry.terminal {
        argv.splice(0..0, ["x-terminal-emulator".to_owned(), "-e".to_owned()]);
    }

    let mut child: Child = spawn_detached(&argv, entry.path.as_deref())?;
    if let Err(error) = move_into_scope(app_id, child.id()) {
        eprintln!("{app_id} runs in the host's cgroup and stops with it: {error:#}");
    }
    let _ = child.wait();
    Ok(())
}

fn spawn_detached(argv: &[String], working_dir: Option<&str>) -> Result<Child> {
    let (program, arguments) = argv.split_first().context("the command is empty")?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // The host's own startup token was used up by its first window.
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        // Keyboard signals aimed at a terminal that started the host must not reach the app.
        .process_group(0);
    if let Some(dir) =
        working_dir.map(PathBuf::from).filter(|dir| dir.is_dir()).or_else(|| env::var_os("HOME").map(PathBuf::from))
    {
        command.current_dir(dir);
    }
    command.spawn().with_context(|| format!("failed to run {program}"))
}

fn activate_over_dbus(app_id: &str) -> Result<()> {
    let platform_data: HashMap<&str, Value<'_>> = HashMap::new();
    dbus::session()?
        .call_method(
            Some(app_id),
            dbus_object_path(app_id).as_str(),
            Some("org.freedesktop.Application"),
            "Activate",
            &(platform_data,),
        )
        .context("org.freedesktop.Application.Activate failed")?;
    Ok(())
}

fn move_into_scope(app_id: &str, pid: u32) -> Result<()> {
    let properties: Vec<(&str, Value<'_>)> =
        vec![("PIDs", Value::from(vec![pid])), ("CollectMode", Value::from("inactive-or-failed"))];
    let auxiliary: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();

    dbus::session()?
        .call_method(
            Some("org.freedesktop.systemd1"),
            "/org/freedesktop/systemd1",
            Some("org.freedesktop.systemd1.Manager"),
            "StartTransientUnit",
            &(scope_unit_name(app_id, pid), "fail", properties, auxiliary),
        )
        .context("failed to start a transient systemd scope")?;
    Ok(())
}

/// Unit name for a launched app, `app-RustyStart-<escaped app ID>-<pid>.scope`. `-` separates the
/// fields, so systemd escaping turns it (and anything outside `[A-Za-z0-9:_.]`) into `\xNN`.
fn scope_unit_name(app_id: &str, pid: u32) -> String {
    let mut escaped = String::with_capacity(app_id.len());
    for byte in app_id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.') {
            escaped.push(char::from(byte));
        } else {
            let _ = write!(escaped, "\\x{byte:02x}");
        }
    }
    format!("app-{SCOPE_LAUNCHER}-{escaped}-{pid}.scope")
}

/// Object path of a D-Bus activatable app: its ID with `.` as `/` and `-` as `_`.
fn dbus_object_path(app_id: &str) -> String {
    format!("/{}", app_id.replace('.', "/").replace('-', "_"))
}

/// Stops the watcher thread when dropped.
pub struct AppWatcher {
    stop: OwnedFd,
    thread: Option<JoinHandle<()>>,
}

impl Drop for AppWatcher {
    fn drop(&mut self) {
        let _ = rustix::io::write(&self.stop, &1_u64.to_ne_bytes());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Rebuilds the app list on a background thread whenever an applications directory changes, and
/// passes it to `on_change` on that thread. Directories created after startup are not watched.
pub fn watch(on_change: impl Fn(Vec<App>) + Send + 'static) -> Result<AppWatcher> {
    let changes: OwnedFd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)
        .context("failed to create an inotify instance")?;
    let watch_flags = inotify::WatchFlags::CREATE
        | inotify::WatchFlags::DELETE
        | inotify::WatchFlags::CLOSE_WRITE
        | inotify::WatchFlags::MOVED_FROM
        | inotify::WatchFlags::MOVED_TO
        | inotify::WatchFlags::ATTRIB;
    let watched: usize = applications_dirs()
        .iter()
        .flat_map(|dir| std::iter::once(dir.clone()).chain(subdirectories(dir)))
        .filter(|dir| inotify::add_watch(&changes, dir, watch_flags).is_ok())
        .count();
    if watched == 0 {
        bail!("no applications directory could be watched");
    }

    let stop: OwnedFd =
        rustix::event::eventfd(0, EventfdFlags::CLOEXEC).context("failed to create the stop eventfd")?;
    let thread_stop: OwnedFd = stop.try_clone().context("failed to share the stop eventfd")?;
    let thread: JoinHandle<()> = thread::Builder::new()
        .name("rusty-start-app-watch".into())
        .spawn(move || run_watcher(&changes, &thread_stop, &on_change))
        .context("failed to spawn the app watcher thread")?;

    Ok(AppWatcher { stop, thread: Some(thread) })
}

fn run_watcher(changes: &OwnedFd, stop: &OwnedFd, on_change: &impl Fn(Vec<App>)) {
    let debounce = Timespec { tv_sec: DEBOUNCE.as_secs() as i64, tv_nsec: i64::from(DEBOUNCE.subsec_nanos()) };
    loop {
        match wait_readable(changes, stop, None) {
            Ok(Readable::Changes) => {}
            Ok(Readable::Stop) => return,
            Err(error) => {
                eprintln!("installed apps will not refresh until restart: {error:#}");
                return;
            }
        }

        // Keep waiting while changes keep arriving, then rebuild once.
        loop {
            drain(changes);
            match wait_readable(changes, stop, Some(&debounce)) {
                Ok(Readable::Changes) => {}
                Ok(Readable::Stop) => return,
                Err(_) => break,
            }
        }

        match load() {
            Ok(apps) => on_change(apps),
            Err(error) => eprintln!("failed to rebuild the app list: {error:#}"),
        }
    }
}

enum Readable {
    Changes,
    Stop,
}

/// Waits until `changes` or `stop` is readable. A timeout is reported as an error.
fn wait_readable(changes: &OwnedFd, stop: &OwnedFd, timeout: Option<&Timespec>) -> Result<Readable> {
    let mut fds = [PollFd::new(changes, PollFlags::IN), PollFd::new(stop, PollFlags::IN)];
    let ready: usize = loop {
        match rustix::event::poll(&mut fds, timeout) {
            Err(rustix::io::Errno::INTR) => {}
            result => break result.context("failed to poll for app changes")?,
        }
    };
    if ready == 0 {
        bail!("timed out");
    }
    if !fds[1].revents().is_empty() {
        return Ok(Readable::Stop);
    }
    Ok(Readable::Changes)
}

fn drain(changes: &impl AsFd) {
    let mut buffer = [0_u8; 4096];
    while rustix::io::read(changes, &mut buffer).is_ok_and(|read| read > 0) {}
}

/// `$XDG_DATA_HOME/applications` followed by `$XDG_DATA_DIRS/*/applications`, without duplicates.
fn applications_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in data_dirs().into_iter().map(|dir| dir.join("applications")) {
        if !dirs.contains(&dir) && dir.is_dir() {
            dirs.push(dir);
        }
    }
    dirs
}

fn data_dirs() -> Vec<PathBuf> {
    let home: Option<PathBuf> = env::var_os("HOME").map(PathBuf::from);
    let data_home: Option<PathBuf> = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| home.join(".local/share")));
    let data_dirs: String = env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());

    data_home.into_iter().chain(data_dirs.split(':').map(PathBuf::from).filter(|dir| dir.is_absolute())).collect()
}

/// Icon base directories in the specification's order: `~/.icons`, then `icons` under each data
/// directory.
fn icon_base_dirs() -> Vec<PathBuf> {
    let legacy: Option<PathBuf> = env::var_os("HOME").map(|home| PathBuf::from(home).join(".icons"));
    legacy.into_iter().chain(data_dirs().into_iter().map(|dir| dir.join("icons"))).collect()
}

/// `.desktop` files under `dir`, recursively, in a stable order.
fn desktop_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut pending: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            if child.is_dir() {
                pending.push(child);
            } else if child.extension().is_some_and(|extension| extension == "desktop") {
                files.push(child);
            }
        }
    }
    files
}

fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.is_dir()).collect())
        .unwrap_or_default()
}

/// Resolves `TryExec`: an absolute path, or a name on `PATH`, that is an executable file.
fn program_exists(program: &str) -> bool {
    let is_executable = |path: &Path| {
        path.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return is_executable(Path::new(program));
    }
    env::var_os("PATH").is_some_and(|path| env::split_paths(&path).any(|dir| is_executable(&dir.join(program))))
}

fn current_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default()
}

fn current_desktops() -> Vec<String> {
    env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|desktop| !desktop.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Plasma's icon theme from `kdeglobals`, else GTK's from `settings.ini`, else `hicolor`. Plasma
/// writes only non-default settings, so an existing `kdeglobals` without a theme means Breeze.
fn current_icon_theme() -> String {
    let config_home: Option<PathBuf> = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let Some(config_home) = config_home else {
        return "hicolor".to_owned();
    };

    let kdeglobals: Option<String> = fs::read_to_string(config_home.join("kdeglobals")).ok();
    if let Some(theme) = kdeglobals.as_deref().and_then(|contents| icon_theme::ini_value(contents, "Icons", "Theme")) {
        return theme;
    }
    if let Some(theme) = fs::read_to_string(config_home.join("gtk-3.0/settings.ini"))
        .ok()
        .and_then(|contents| icon_theme::ini_value(&contents, "Settings", "gtk-icon-theme-name"))
    {
        return theme;
    }
    if kdeglobals.is_some() { "breeze".to_owned() } else { "hicolor".to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_names_escape_dashes_and_other_reserved_characters() {
        assert_eq!(scope_unit_name("org.kde.konsole", 42), "app-RustyStart-org.kde.konsole-42.scope");
        assert_eq!(scope_unit_name("firefox-esr", 7), "app-RustyStart-firefox\\x2desr-7.scope");
        assert_eq!(scope_unit_name("my app", 1), "app-RustyStart-my\\x20app-1.scope");
    }

    #[test]
    fn dbus_object_paths_map_dots_and_dashes() {
        assert_eq!(dbus_object_path("org.gnome.Nautilus"), "/org/gnome/Nautilus");
        assert_eq!(dbus_object_path("org.example.my-app"), "/org/example/my_app");
    }
}
