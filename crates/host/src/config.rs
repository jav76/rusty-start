//! Persistent configuration: where the Liquid-Glass Card sits inside the full-monitor Fog Veil, how
//! the veil is drawn, and whether the Windows key opens the menu.
//!
//! Defaults to anchoring the `640x720` Liquid-Glass Card at the middle-left of the active monitor,
//! and persists changes to `%APPDATA%\RustyStart\config.toml` (or `$XDG_CONFIG_HOME/rusty-start/config.toml`).

use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::{logging, persist};

/// Logical width of the floating Liquid-Glass Card.
pub const CARD_WIDTH: f32 = 640.0;

/// Logical height of the floating Liquid-Glass Card.
pub const CARD_HEIGHT: f32 = 720.0;

/// Default inset from the screen edge in logical pixels for edge-anchored presets.
pub const DEFAULT_EDGE_MARGIN: f32 = 48.0;

/// Highest `veil_blur`, which draws the full Fog Veil blur.
pub const MAX_VEIL_BLUR: u8 = 100;

/// How long the writer waits for further changes before saving, so dragging the blur slider writes
/// the file once instead of on every step.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

/// Alignment preset positioning the `640x720` Liquid-Glass Card inside the monitor's Fog Veil.
/// Variants are in row-major order of the Settings Page's 3x3 picker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    #[default]
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    Start,
    Middle,
    End,
}

impl CardAnchor {
    pub const ALL: [Self; 9] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::MiddleLeft,
        Self::Center,
        Self::MiddleRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];

    /// Position in the 3x3 picker, row-major from the top-left cell.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// Serialized identifier used in `config.toml`.
    pub fn as_key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopCenter => "top-center",
            Self::TopRight => "top-right",
            Self::MiddleLeft => "middle-left",
            Self::Center => "center",
            Self::MiddleRight => "middle-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomCenter => "bottom-center",
            Self::BottomRight => "bottom-right",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "top-left" => Some(Self::TopLeft),
            "top-center" | "top" => Some(Self::TopCenter),
            "top-right" => Some(Self::TopRight),
            "middle-left" | "left" => Some(Self::MiddleLeft),
            "center" | "middle" => Some(Self::Center),
            "middle-right" | "right" => Some(Self::MiddleRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-center" | "bottom" => Some(Self::BottomCenter),
            "bottom-right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    fn column(self) -> Placement {
        match self.index() % 3 {
            0 => Placement::Start,
            1 => Placement::Middle,
            _ => Placement::End,
        }
    }

    fn row(self) -> Placement {
        match self.index() / 3 {
            0 => Placement::Start,
            1 => Placement::Middle,
            _ => Placement::End,
        }
    }

    /// Returns the top-left `(x, y)` coordinates of the `640x720` card in logical pixels within a
    /// monitor of `(monitor_width, monitor_height)` logical pixels.
    pub fn card_origin(self, monitor_width: f32, monitor_height: f32, edge_margin: f32) -> (f32, f32) {
        let max_x: f32 = (monitor_width - CARD_WIDTH).max(0.0);
        let max_y: f32 = (monitor_height - CARD_HEIGHT).max(0.0);
        let margin: f32 = edge_margin.max(0.0);

        (place(self.column(), max_x, margin), place(self.row(), max_y, margin))
    }
}

/// Offset along one axis with `max` free space, keeping edge placements `margin` from the edge.
fn place(placement: Placement, max: f32, margin: f32) -> f32 {
    match placement {
        Placement::Start => margin.min(max),
        Placement::Middle => (max * 0.5).round(),
        Placement::End => (max - margin).max(0.0),
    }
}

/// How the Fog Veil around the Liquid-Glass Card is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VeilMode {
    /// Blurred and dimmed desktop snapshot.
    #[default]
    Fog,
    /// Dimmed live desktop without blur.
    Dim,
    /// No veil: only the card is drawn.
    Off,
}

impl VeilMode {
    pub const ALL: [Self; 3] = [Self::Fog, Self::Dim, Self::Off];

    /// Position in the Settings Page's segmented control, also the shader's `u_veil_mode`.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Self::Fog => "fog",
            Self::Dim => "dim",
            Self::Off => "off",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "fog" => Some(Self::Fog),
            "dim" => Some(Self::Dim),
            "off" | "none" => Some(Self::Off),
            _ => None,
        }
    }
}

/// User configuration loaded from `config.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub card_anchor: CardAnchor,
    pub edge_margin: f32,
    pub veil_mode: VeilMode,
    /// Fog Veil blur strength from 0 (dim only) to `MAX_VEIL_BLUR`.
    pub veil_blur: u8,
    pub use_win_key: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            card_anchor: CardAnchor::MiddleLeft,
            edge_margin: DEFAULT_EDGE_MARGIN,
            veil_mode: VeilMode::Fog,
            veil_blur: MAX_VEIL_BLUR,
            use_win_key: true,
        }
    }
}

/// Whether settings changes may be written back to `config.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Saving {
    Allowed,
    /// An unreadable file could not be moved aside, so saving would overwrite it.
    Blocked(String),
}

/// A loaded configuration and whether it is safe to save over its file.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub config: Config,
    pub saving: Saving,
}

const KNOWN_KEYS: [&str; 5] = ["card_anchor", "edge_margin", "veil_mode", "veil_blur", "use_win_key"];

impl Config {
    /// Loads the configuration. A missing file means the defaults. A file that cannot be read, for
    /// example one saved as UTF-16, is moved aside with `set_aside` so a later save cannot destroy
    /// it, and the defaults apply; if even that fails, saving is blocked.
    pub fn load() -> Loaded {
        let Some(path) = config_file_path() else {
            return Loaded { config: Self::default(), saving: Saving::Allowed };
        };

        match fs::read_to_string(&path) {
            Ok(contents) => Loaded { config: Self::parse_toml(&contents), saving: Saving::Allowed },
            Err(error) if error.kind() == ErrorKind::NotFound => {
                Loaded { config: Self::default(), saving: Saving::Allowed }
            }
            Err(error) => match set_aside(&path) {
                Ok(moved_to) => {
                    log::error!(
                        "config.toml could not be read ({error}); it was moved to {} and the defaults apply",
                        moved_to.display()
                    );
                    Loaded { config: Self::default(), saving: Saving::Allowed }
                }
                Err(set_aside_error) => {
                    let reason: String =
                        format!("config.toml could not be read ({error}) or moved aside: {set_aside_error:#}");
                    Loaded { config: Self::default(), saving: Saving::Blocked(reason) }
                }
            },
        }
    }

    /// Parses the file leniently: unknown keys and values out of range are logged and skipped, so
    /// a typo never discards the other settings.
    pub fn parse_toml(contents: &str) -> Self {
        let mut config = Self::default();

        for raw_line in strip_bom(contents).lines() {
            let Some((key, value)) = entry(raw_line) else {
                continue;
            };

            let accepted: bool = match key {
                "card_anchor" => CardAnchor::from_key(value).map(|anchor| config.card_anchor = anchor).is_some(),
                "edge_margin" => match value.parse::<f32>() {
                    Ok(margin) if margin.is_finite() && (0.0..=512.0).contains(&margin) => {
                        config.edge_margin = margin;
                        true
                    }
                    _ => false,
                },
                "veil_mode" => VeilMode::from_key(value).map(|mode| config.veil_mode = mode).is_some(),
                "veil_blur" => value.parse::<u8>().map(|blur| config.veil_blur = blur.min(MAX_VEIL_BLUR)).is_ok(),
                "use_win_key" => parse_bool(value).map(|enabled| config.use_win_key = enabled).is_some(),
                _ => {
                    log::warn!("config.toml: unknown key {key:?} is ignored");
                    continue;
                }
            };
            if !accepted {
                log::warn!("config.toml: {key} = {value:?} is not a valid value and is ignored");
            }
        }

        config
    }

    /// Renders the file to save. Given the file's current contents, every line the host does not
    /// own (comments, blank lines, keys from other versions) is kept as it is, each known key is
    /// updated in place with its trailing comment, duplicates collapse into the first, and known
    /// keys the file lacks are appended. Without contents, writes the commented template.
    pub fn render(self, existing: Option<&str>) -> String {
        let Some(existing) = existing else {
            return self.template();
        };

        let mut written: Vec<&str> = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        for raw_line in strip_bom(existing).lines() {
            match entry(raw_line) {
                Some((key, _)) if KNOWN_KEYS.contains(&key) => {
                    if written.contains(&key) {
                        continue;
                    }
                    written.push(key);
                    let comment: &str = raw_line.find('#').map_or("", |index| &raw_line[index..]);
                    let separator: &str = if comment.is_empty() { "" } else { " " };
                    lines.push(format!("{key} = {}{separator}{comment}", self.value(key)));
                }
                _ => lines.push(raw_line.to_owned()),
            }
        }
        for key in KNOWN_KEYS.iter().filter(|key| !written.contains(key)) {
            lines.push(format!("{key} = {}", self.value(key)));
        }

        let mut rendered: String = lines.join("\n");
        rendered.push('\n');
        rendered
    }

    fn template(self) -> String {
        format!(
            "# Rusty Start configuration\n\
             # Options: \"top-left\", \"top-center\", \"top-right\", \"middle-left\" (default), \"center\",\n\
             # \"middle-right\", \"bottom-left\", \"bottom-center\", \"bottom-right\"\n\
             card_anchor = {}\n\
             edge_margin = {}\n\
             # Options: \"fog\" (default), \"dim\", \"off\"\n\
             veil_mode = {}\n\
             # Fog Veil blur strength from 0 to 100\n\
             veil_blur = {}\n\
             use_win_key = {}\n",
            self.value("card_anchor"),
            self.value("edge_margin"),
            self.value("veil_mode"),
            self.value("veil_blur"),
            self.value("use_win_key"),
        )
    }

    /// The TOML value of a known key.
    fn value(self, key: &str) -> String {
        match key {
            "card_anchor" => format!("\"{}\"", self.card_anchor.as_key()),
            "edge_margin" => self.edge_margin.to_string(),
            "veil_mode" => format!("\"{}\"", self.veil_mode.as_key()),
            "veil_blur" => self.veil_blur.to_string(),
            "use_win_key" => self.use_win_key.to_string(),
            _ => unreachable!("`value` is called with entries of KNOWN_KEYS only"),
        }
    }
}

/// Notepad on Windows writes a byte-order mark, which is not part of the first key.
fn strip_bom(contents: &str) -> &str {
    contents.strip_prefix('\u{feff}').unwrap_or(contents)
}

/// The `key = value` of a line, without its comment, quotes and surrounding whitespace.
fn entry(line: &str) -> Option<(&str, &str)> {
    let content: &str = line.split('#').next().unwrap_or("").trim();
    let (key, value) = content.split_once('=')?;
    Some((key.trim(), value.trim().trim_matches('"').trim_matches('\'')))
}

/// Moves a file the host cannot use to `<name>.broken-<timestamp>` next to it and returns the new
/// path, so the user's data survives and the host can start over.
pub fn set_aside(path: &Path) -> Result<PathBuf> {
    let file_name: String = path.file_name().and_then(|name| name.to_str()).unwrap_or("file").to_owned();
    let moved_to: PathBuf =
        path.with_file_name(format!("{file_name}.broken-{}", logging::file_stamp(logging::unix_seconds())));
    fs::rename(path, &moved_to)
        .with_context(|| format!("failed to move {file_name} aside to {}", moved_to.display()))?;
    Ok(moved_to)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Directory holding `config.toml` and `usage.tsv`: `%APPDATA%\RustyStart`.
#[cfg(windows)]
pub fn data_dir() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("RustyStart"))
}

/// Directory holding `config.toml` and `usage.tsv`: `$XDG_CONFIG_HOME/rusty-start`, or
/// `~/.config/rusty-start` when it is unset or not absolute.
#[cfg(not(windows))]
pub fn data_dir() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        && xdg.is_absolute()
    {
        return Some(xdg.join("rusty-start"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("rusty-start"))
}

fn config_file_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("config.toml"))
}

enum Message {
    Save(Config),
    /// Write whatever is pending now and reply, for a session that is about to end.
    Flush(Sender<()>),
    Stop,
}

/// Saves the configuration on a background thread, coalescing changes that arrive within
/// `SAVE_DEBOUNCE`. Dropping it writes any pending change and then stops the thread.
pub struct Writer {
    sender: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn spawn() -> Result<Self> {
        let path: PathBuf = config_file_path().context("failed to determine the configuration directory")?;
        let (sender, receiver) = mpsc::channel::<Message>();

        let thread: JoinHandle<()> = thread::Builder::new()
            .name("rusty-start-config".into())
            .spawn(move || {
                run_writer(&receiver, SAVE_DEBOUNCE, |config| {
                    // Re-read just before writing, so edits made by hand while the host runs survive.
                    let existing: Option<String> = fs::read_to_string(&path).ok();
                    if let Err(error) = write_replacing(&path, &config.render(existing.as_deref())) {
                        log::error!("failed to save config.toml: {error:#}");
                    }
                });
            })
            .context("failed to spawn the config writer thread")?;

        let flush_sender: Sender<Message> = sender.clone();
        persist::register(move || flush(&flush_sender));
        Ok(Self { sender, thread: Some(thread) })
    }

    pub fn save(&self, config: Config) {
        if self.sender.send(Message::Save(config)).is_err() {
            log::error!("failed to queue a config save: the writer thread has exited");
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Asks the writer to save now and waits for it, for at most `persist::FLUSH_TIMEOUT`. A writer
/// that has already stopped has nothing pending.
fn flush(sender: &Sender<Message>) {
    let (reply, done) = mpsc::channel::<()>();
    if sender.send(Message::Flush(reply)).is_ok() {
        let _ = done.recv_timeout(persist::FLUSH_TIMEOUT);
    }
}

fn run_writer(receiver: &Receiver<Message>, debounce: Duration, mut write: impl FnMut(Config)) {
    loop {
        let mut config: Config = match receiver.recv() {
            Ok(Message::Save(config)) => config,
            Ok(Message::Flush(reply)) => {
                let _ = reply.send(());
                continue;
            }
            Ok(Message::Stop) | Err(_) => break,
        };

        let mut stop: bool = false;
        let mut flush_reply: Option<Sender<()>> = None;
        loop {
            match receiver.recv_timeout(debounce) {
                Ok(Message::Save(newer)) => config = newer,
                Ok(Message::Flush(reply)) => {
                    flush_reply = Some(reply);
                    break;
                }
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    stop = true;
                    break;
                }
                Err(RecvTimeoutError::Timeout) => break,
            }
        }

        write(config);
        if let Some(reply) = flush_reply {
            let _ = reply.send(());
        }
        if stop {
            break;
        }
    }
}

/// Writes a temporary file and renames it over `path`, so a crash never leaves a truncated file.
fn write_replacing(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create the configuration directory")?;
    }

    let temporary: PathBuf = path.with_extension("toml.tmp");
    fs::write(&temporary, contents).context("failed to write the temporary config file")?;
    fs::rename(&temporary, path).context("failed to replace config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_anchor_is_middle_left() {
        let config = Config::default();

        assert_eq!(config.card_anchor, CardAnchor::MiddleLeft);
        assert!((config.edge_margin - DEFAULT_EDGE_MARGIN).abs() < f32::EPSILON);
        assert_eq!(config.veil_mode, VeilMode::Fog);
        assert_eq!(config.veil_blur, MAX_VEIL_BLUR);
        assert!(config.use_win_key);
    }

    #[test]
    fn computes_origins_for_every_anchor_across_resolutions() {
        let full_hd: Vec<(f32, f32)> =
            CardAnchor::ALL.iter().map(|anchor| anchor.card_origin(1920.0, 1080.0, 48.0)).collect();
        assert_eq!(
            full_hd,
            [
                (48.0, 48.0),
                (640.0, 48.0),
                (1232.0, 48.0),
                (48.0, 180.0),
                (640.0, 180.0),
                (1232.0, 180.0),
                (48.0, 312.0),
                (640.0, 312.0),
                (1232.0, 312.0),
            ]
        );
        assert_eq!(CardAnchor::BottomLeft.card_origin(2560.0, 1440.0, 48.0), (48.0, 672.0));
        assert_eq!(CardAnchor::BottomCenter.card_origin(2560.0, 1440.0, 48.0), (960.0, 672.0));
    }

    #[test]
    fn a_monitor_smaller_than_the_card_pins_it_to_the_top_left() {
        assert_eq!(CardAnchor::BottomRight.card_origin(600.0, 600.0, 48.0), (0.0, 0.0));
    }

    #[test]
    fn anchor_indexes_round_trip_in_row_major_order() {
        for (index, anchor) in CardAnchor::ALL.iter().enumerate() {
            assert_eq!(anchor.index(), index);
            assert_eq!(CardAnchor::from_index(index), Some(*anchor));
        }
        assert_eq!(CardAnchor::from_index(9), None);
    }

    #[test]
    fn older_anchor_keys_and_aliases_still_parse() {
        assert_eq!(CardAnchor::from_key("middle_left"), Some(CardAnchor::MiddleLeft));
        assert_eq!(CardAnchor::from_key("left"), Some(CardAnchor::MiddleLeft));
        assert_eq!(CardAnchor::from_key("middle"), Some(CardAnchor::Center));
        assert_eq!(CardAnchor::from_key("bottom_left"), Some(CardAnchor::BottomLeft));
        assert_eq!(CardAnchor::from_key("bottom"), Some(CardAnchor::BottomCenter));
        for anchor in CardAnchor::ALL {
            assert_eq!(CardAnchor::from_key(anchor.as_key()), Some(anchor));
        }
    }

    #[test]
    fn round_trips_toml_serialization() {
        let original = Config {
            card_anchor: CardAnchor::TopRight,
            edge_margin: 64.5,
            veil_mode: VeilMode::Dim,
            veil_blur: 35,
            use_win_key: false,
        };

        assert_eq!(Config::parse_toml(&original.render(None)), original);
        assert_eq!(Config::parse_toml(&original.render(Some("veil_blur = 1\n"))), original);
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_first_key() {
        let config = Config::parse_toml("\u{feff}veil_blur = 40\n");

        assert_eq!(config.veil_blur, 40);
    }

    #[test]
    fn unknown_keys_and_comments_survive_a_save() {
        let existing =
            "# my notes\nfile_search_folders = [\"~/Documents\"]\n\nveil_blur = 40 # keep it soft\nveil_mode = 'dim'\n";
        let config = Config { veil_blur: 55, ..Config::default() };

        let rendered: String = config.render(Some(existing));

        assert_eq!(
            rendered,
            "# my notes\nfile_search_folders = [\"~/Documents\"]\n\nveil_blur = 55 # keep it soft\nveil_mode = \"fog\"\n\
             card_anchor = \"middle-left\"\nedge_margin = 48\nuse_win_key = true\n"
        );
        assert_eq!(Config::parse_toml(&rendered), config);
    }

    #[test]
    fn duplicate_keys_collapse_into_the_first() {
        let rendered: String = Config::default().render(Some("veil_blur = 1\nveil_blur = 2\n"));

        assert_eq!(rendered.matches("veil_blur").count(), 1);
        assert!(rendered.starts_with("veil_blur = 100\n"));
    }

    #[test]
    fn a_byte_order_mark_is_dropped_when_saving() {
        let rendered: String = Config::default().render(Some("\u{feff}veil_blur = 1\n"));

        assert!(rendered.starts_with("veil_blur = 100\n"));
    }

    #[test]
    fn keys_from_older_versions_are_ignored() {
        let config = Config::parse_toml("use_taskbar_hook = false\nveil_blur = 40\n");

        assert_eq!(config, Config { veil_blur: 40, ..Config::default() });
    }

    #[test]
    fn out_of_range_veil_values_are_clamped_or_ignored() {
        let config = Config::parse_toml("veil_blur = 250\nveil_mode = \"sparkle\"\n");
        assert_eq!(config.veil_blur, MAX_VEIL_BLUR);
        assert_eq!(config.veil_mode, VeilMode::Fog);

        let config = Config::parse_toml("veil_blur = 40\nveil_blur = -3\n");
        assert_eq!(config.veil_blur, 40);
    }

    #[test]
    fn writer_saves_only_the_newest_of_queued_changes() {
        let (sender, receiver) = mpsc::channel::<Message>();
        for veil_blur in [10, 20, 30] {
            sender.send(Message::Save(Config { veil_blur, ..Config::default() })).unwrap();
        }
        sender.send(Message::Stop).unwrap();

        let mut written: Vec<u8> = Vec::new();
        run_writer(&receiver, Duration::from_secs(5), |config| written.push(config.veil_blur));

        assert_eq!(written, [30]);
    }

    #[test]
    fn a_flush_during_the_debounce_writes_at_once_and_replies() {
        let (sender, receiver) = mpsc::channel::<Message>();
        let (reply, done) = mpsc::channel::<()>();
        sender.send(Message::Save(Config { veil_blur: 10, ..Config::default() })).unwrap();
        sender.send(Message::Flush(reply)).unwrap();
        sender.send(Message::Stop).unwrap();

        let mut written: Vec<u8> = Vec::new();
        run_writer(&receiver, Duration::from_secs(5), |config| written.push(config.veil_blur));

        assert_eq!(written, [10]);
        assert!(done.try_recv().is_ok());
    }

    #[test]
    fn a_flush_with_nothing_pending_replies_without_writing() {
        let (sender, receiver) = mpsc::channel::<Message>();
        let (reply, done) = mpsc::channel::<()>();
        sender.send(Message::Flush(reply)).unwrap();
        sender.send(Message::Stop).unwrap();

        let mut written: Vec<u8> = Vec::new();
        run_writer(&receiver, Duration::from_secs(5), |config| written.push(config.veil_blur));

        assert_eq!(written, Vec::<u8>::new());
        assert!(done.try_recv().is_ok());
    }
}
