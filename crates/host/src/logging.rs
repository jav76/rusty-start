//! The host's only log sink. Every `log` message goes to stderr and, when the configuration folder
//! is writable, to a bounded `rusty-start.log` next to `config.toml`, so Windows release builds,
//! which run without a console, still leave a trace. Panics are appended to the same file.
//!
//! Messages are formatted on the calling thread and written to the file by the `rusty-start-log`
//! thread, so logging puts no disk I/O on the open path.

use std::fmt::{self, Arguments};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::config;

pub const LOG_FILE_NAME: &str = "rusty-start.log";
const ROTATED_FILE_NAME: &str = "rusty-start.log.1";

/// Size above which the log file is moved aside at startup.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// Environment variable that raises or lowers the level: `debug`, `info`, `warn` or `error`.
pub const LEVEL_VARIABLE: &str = "RUSTY_START_LOG";

/// Prefix of this binary's own log targets (`module_path!()` of every module in the crate).
const OWN_TARGET_PREFIX: &str = "rusty_start";

static LOGGER: Logger = Logger;
static SENDER: OnceLock<Sender<Message>> = OnceLock::new();

enum Message {
    Line(String),
    Stop,
}

/// Stops the writer thread when dropped, after the lines logged so far are on disk.
pub struct LogGuard {
    thread: Option<JoinHandle<()>>,
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        if let Some(sender) = SENDER.get() {
            let _ = sender.send(Message::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Installs the logger and the panic hook. Returns `None` when the log file cannot be opened; stderr
/// logging works either way.
pub fn init() -> Option<LogGuard> {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(level_filter(cfg!(debug_assertions), std::env::var(LEVEL_VARIABLE).ok().as_deref()));

    let path: Option<PathBuf> = log_file_path();
    let guard: Option<LogGuard> = path.as_deref().and_then(|path| match spawn_writer(path) {
        Ok(thread) => Some(LogGuard { thread: Some(thread) }),
        Err(error) => {
            log::warn!("the log file {} is unavailable: {error:#}", path.display());
            None
        }
    });
    install_panic_hook(path);
    guard
}

/// Where the host writes its log: `rusty-start.log` in the configuration folder.
pub fn log_file_path() -> Option<PathBuf> {
    config::data_dir().map(|dir| dir.join(LOG_FILE_NAME))
}

fn spawn_writer(path: &Path) -> Result<JoinHandle<()>> {
    let file: File = open_log_file(path)?;
    let (sender, receiver) = mpsc::channel::<Message>();
    let thread: JoinHandle<()> = thread::Builder::new()
        .name("rusty-start-log".into())
        .spawn(move || write_lines(file, &receiver))
        .context("failed to spawn the log writer thread")?;
    SENDER.set(sender).ok().context("logging was initialized twice")?;
    Ok(thread)
}

fn open_log_file(path: &Path) -> Result<File> {
    let directory: &Path = path.parent().context("the log file has a parent directory")?;
    fs::create_dir_all(directory).context("failed to create the configuration directory")?;
    if let Ok(metadata) = fs::metadata(path)
        && should_rotate(metadata.len(), MAX_LOG_BYTES)
    {
        fs::rename(path, directory.join(ROTATED_FILE_NAME)).context("failed to rotate the log file")?;
    }
    OpenOptions::new().create(true).append(true).open(path).context("failed to open the log file")
}

fn write_lines(mut file: File, receiver: &Receiver<Message>) {
    for message in receiver {
        match message {
            // A failed write cannot be logged anywhere, and stderr already has the line.
            Message::Line(line) => {
                let _ = writeln!(file, "{line}");
            }
            Message::Stop => break,
        }
    }
    let _ = file.flush();
}

struct Logger;

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.target().starts_with(OWN_TARGET_PREFIX) || foreign_target_allowed(metadata.level())
    }

    #[expect(clippy::print_stderr, reason = "this is the stderr sink every other module logs through")]
    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line: String =
            format_line(record.level(), record.target(), *record.args(), unix_seconds(), thread::current().name());
        eprintln!("{line}");
        if let Some(sender) = SENDER.get() {
            // The writer thread has stopped; stderr already has the line.
            let _ = sender.send(Message::Line(line));
        }
    }

    fn flush(&self) {}
}

/// Messages from dependencies (winit, zbus, femtovg) pass only when they signal a problem.
fn foreign_target_allowed(level: Level) -> bool {
    level <= Level::Warn
}

fn level_filter(debug_build: bool, setting: Option<&str>) -> LevelFilter {
    match setting.map(str::to_ascii_lowercase).as_deref() {
        Some("debug") => LevelFilter::Debug,
        Some("info") => LevelFilter::Info,
        Some("warn") => LevelFilter::Warn,
        Some("error") => LevelFilter::Error,
        _ if debug_build => LevelFilter::Debug,
        _ => LevelFilter::Info,
    }
}

fn should_rotate(length: u64, limit: u64) -> bool {
    length > limit
}

fn unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

fn format_line(
    level: Level,
    target: &str,
    message: Arguments<'_>,
    unix_secs: u64,
    thread_name: Option<&str>,
) -> String {
    let timestamp: String = format_utc(unix_secs);
    match thread_name {
        Some(name) if name != "main" => format!("{timestamp} {level:<5} {target} [{name}] {message}"),
        _ => format!("{timestamp} {level:<5} {target} {message}"),
    }
}

/// Formats seconds since the Unix epoch as `2026-11-02T14:03:07Z`, without a date dependency.
pub fn format_utc(unix_secs: u64) -> String {
    let (date, seconds_of_day) = (unix_secs / 86_400, unix_secs % 86_400);
    let (year, month, day) = civil_from_days(date);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3600,
        seconds_of_day % 3600 / 60,
        seconds_of_day % 60
    )
}

/// Howard Hinnant's `civil_from_days`, for days since 1970-01-01.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z: u64 = days + 719_468;
    let era: u64 = z / 146_097;
    let day_of_era: u64 = z - era * 146_097;
    let year_of_era: u64 = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year: u64 = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month: u64 = (5 * day_of_year + 2) / 153;
    let day: u64 = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month: u64 = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year: u64 = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

fn install_panic_hook(path: Option<PathBuf>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        let description: String = describe_panic(info);
        let line: String = format_line(
            Level::Error,
            OWN_TARGET_PREFIX,
            format_args!("{description}"),
            unix_seconds(),
            thread::current().name(),
        );
        // The panic itself is reported by the previous hook; the file write is best-effort.
        if let Some(path) = &path
            && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
        {
            let _ = writeln!(file, "{line}");
        }
        show_panic_notice(&description, path.as_deref());
        previous(info);
    }));
}

fn describe_panic(info: &PanicHookInfo<'_>) -> String {
    let payload: &dyn std::any::Any = info.payload();
    let message: &str = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic with a non-string payload");
    match info.location() {
        Some(location) => format!("panic at {}:{}: {message}", location.file(), location.line()),
        None => format!("panic: {message}"),
    }
}

/// Prints a command-line report. Windows release builds have no console, so this attaches to the
/// parent console when there is one and shows a message box otherwise.
#[expect(clippy::print_stdout, reason = "the report is the command's output")]
pub fn print_report(text: &str) {
    if attach_parent_console() {
        println!("{text}");
    } else {
        show_message_box(text, false);
    }
}

/// Reports a failure of the whole command: logged, and shown in a message box where there is no
/// console to see it in.
pub fn report_failure(error: &anyhow::Error) {
    log::error!(target: OWN_TARGET_PREFIX, "{error:#}");
    if !attach_parent_console() {
        show_message_box(&format!("{error:#}"), true);
    }
}

#[cfg(all(windows, not(debug_assertions)))]
fn attach_parent_console() -> bool {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

    // SAFETY: attaching to the parent's console has no preconditions; it fails when there is none.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok()
}

/// Debug builds on Windows and every other platform already have a console.
#[cfg(not(all(windows, not(debug_assertions))))]
fn attach_parent_console() -> bool {
    true
}

#[cfg(windows)]
fn show_message_box(text: &str, is_error: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_TOPMOST, MessageBoxW};
    use windows::core::HSTRING;

    let icon = if is_error { MB_ICONERROR } else { MB_ICONINFORMATION };
    // SAFETY: both strings are valid null-terminated wide strings for the duration of the call.
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from("Rusty Start"), MB_OK | MB_TOPMOST | icon) };
}

#[cfg(not(windows))]
#[expect(clippy::print_stderr, reason = "there is no message box to fall back to")]
fn show_message_box(text: &str, _is_error: bool) {
    eprintln!("{text}");
}

#[cfg(all(windows, not(debug_assertions)))]
fn show_panic_notice(description: &str, path: Option<&Path>) {
    let location: String = path.map_or_else(String::new, |path| format!("\n\nDetails are in {}", path.display()));
    show_message_box(&format!("Rusty Start stopped: {description}{location}"), true);
}

#[cfg(not(all(windows, not(debug_assertions))))]
fn show_panic_notice(_description: &str, _path: Option<&Path>) {}

impl fmt::Debug for LogGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("LogGuard").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_and_a_recent_instant_format_as_utc() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_762_092_187), "2025-11-02T14:03:07Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn lines_carry_the_level_target_and_background_thread_name() {
        let line: String = format_line(Level::Warn, "rusty_start::menu", format_args!("no focus"), 0, Some("main"));
        assert_eq!(line, "1970-01-01T00:00:00Z WARN  rusty_start::menu no focus");

        let line: String =
            format_line(Level::Error, "rusty_start::apps", format_args!("boom"), 60, Some("rusty-start-apps"));
        assert_eq!(line, "1970-01-01T00:01:00Z ERROR rusty_start::apps [rusty-start-apps] boom");
    }

    #[test]
    fn dependencies_log_only_problems() {
        assert!(foreign_target_allowed(Level::Error));
        assert!(foreign_target_allowed(Level::Warn));
        assert!(!foreign_target_allowed(Level::Info));
        assert!(!foreign_target_allowed(Level::Debug));
    }

    #[test]
    fn the_level_comes_from_the_environment_then_the_build_kind() {
        assert_eq!(level_filter(false, None), LevelFilter::Info);
        assert_eq!(level_filter(true, None), LevelFilter::Debug);
        assert_eq!(level_filter(false, Some("DEBUG")), LevelFilter::Debug);
        assert_eq!(level_filter(true, Some("warn")), LevelFilter::Warn);
        assert_eq!(level_filter(false, Some("verbose")), LevelFilter::Info);
    }

    #[test]
    fn the_file_rotates_only_above_the_limit() {
        assert!(!should_rotate(MAX_LOG_BYTES, MAX_LOG_BYTES));
        assert!(should_rotate(MAX_LOG_BYTES + 1, MAX_LOG_BYTES));
    }
}
