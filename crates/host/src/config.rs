//! Persistent configuration: where the Liquid-Glass Card sits inside the full-monitor Fog Veil, how
//! the veil is drawn, and whether the Windows key opens the menu.
//!
//! Defaults to anchoring the `640x720` Liquid-Glass Card at the middle-left of the active monitor,
//! and persists changes to `%APPDATA%\RustyStart\config.toml` (or `$XDG_CONFIG_HOME/rusty-start/config.toml`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};

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
    pub use_taskbar_hook: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            card_anchor: CardAnchor::MiddleLeft,
            edge_margin: DEFAULT_EDGE_MARGIN,
            veil_mode: VeilMode::Fog,
            veil_blur: MAX_VEIL_BLUR,
            use_win_key: true,
            use_taskbar_hook: true,
        }
    }
}

impl Config {
    /// Loads configuration from disk, or returns `Config::default()` if none exists yet.
    pub fn load() -> Self {
        let Some(path) = config_file_path() else {
            return Self::default();
        };

        match fs::read_to_string(path) {
            Ok(contents) => Self::parse_toml(&contents),
            Err(_) => Self::default(),
        }
    }

    pub fn parse_toml(contents: &str) -> Self {
        let mut config = Self::default();

        for raw_line in contents.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');

            match key {
                "card_anchor" => {
                    if let Some(anchor) = CardAnchor::from_key(value) {
                        config.card_anchor = anchor;
                    }
                }
                "edge_margin" => {
                    if let Ok(margin) = value.parse::<f32>()
                        && margin.is_finite()
                        && (0.0..=512.0).contains(&margin)
                    {
                        config.edge_margin = margin;
                    }
                }
                "veil_mode" => {
                    if let Some(mode) = VeilMode::from_key(value) {
                        config.veil_mode = mode;
                    }
                }
                "veil_blur" => {
                    if let Ok(blur) = value.parse::<u8>() {
                        config.veil_blur = blur.min(MAX_VEIL_BLUR);
                    }
                }
                "use_win_key" => {
                    if let Some(enabled) = parse_bool(value) {
                        config.use_win_key = enabled;
                    }
                }
                "use_taskbar_hook" => {
                    if let Some(enabled) = parse_bool(value) {
                        config.use_taskbar_hook = enabled;
                    }
                }
                _ => {}
            }
        }

        config
    }

    pub fn to_toml(self) -> String {
        format!(
            "# Rusty Start configuration\n\
             # Options: \"top-left\", \"top-center\", \"top-right\", \"middle-left\" (default), \"center\",\n\
             # \"middle-right\", \"bottom-left\", \"bottom-center\", \"bottom-right\"\n\
             card_anchor = \"{}\"\n\
             edge_margin = {}\n\
             # Options: \"fog\" (default), \"dim\", \"off\"\n\
             veil_mode = \"{}\"\n\
             # Fog Veil blur strength from 0 to 100\n\
             veil_blur = {}\n\
             use_win_key = {}\n\
             use_taskbar_hook = {}\n",
            self.card_anchor.as_key(),
            self.edge_margin as u32,
            self.veil_mode.as_key(),
            self.veil_blur,
            self.use_win_key,
            self.use_taskbar_hook,
        )
    }
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
                    if let Err(error) = write_replacing(&path, &config.to_toml()) {
                        eprintln!("failed to save config.toml: {error:#}");
                    }
                });
            })
            .context("failed to spawn the config writer thread")?;

        Ok(Self { sender, thread: Some(thread) })
    }

    pub fn save(&self, config: Config) {
        if self.sender.send(Message::Save(config)).is_err() {
            eprintln!("failed to queue a config save: the writer thread has exited");
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

fn run_writer(receiver: &Receiver<Message>, debounce: Duration, mut write: impl FnMut(Config)) {
    while let Ok(Message::Save(mut config)) = receiver.recv() {
        let mut stop = false;
        loop {
            match receiver.recv_timeout(debounce) {
                Ok(Message::Save(newer)) => config = newer,
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    stop = true;
                    break;
                }
                Err(RecvTimeoutError::Timeout) => break,
            }
        }

        write(config);
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
        assert!(config.use_taskbar_hook);
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
            edge_margin: 64.0,
            veil_mode: VeilMode::Dim,
            veil_blur: 35,
            use_win_key: false,
            use_taskbar_hook: false,
        };
        let parsed = Config::parse_toml(&original.to_toml());

        assert_eq!(parsed, original);
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
}
