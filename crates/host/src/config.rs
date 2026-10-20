//! Persistent configuration for the Start menu card placement inside the full-monitor Fog Veil.
//!
//! Defaults to anchoring the `640x720` Liquid-Glass Card at the middle-left of the active monitor,
//! and persists changes to `%APPDATA%\RustyStart\config.toml` (or `$XDG_CONFIG_HOME/rusty-start/config.toml`).

use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Logical width of the floating Liquid-Glass Card.
pub const CARD_WIDTH: f32 = 640.0;

/// Logical height of the floating Liquid-Glass Card.
pub const CARD_HEIGHT: f32 = 720.0;

/// Default inset from the screen edge in logical pixels for edge-anchored presets.
pub const DEFAULT_EDGE_MARGIN: f32 = 48.0;

/// Alignment preset positioning the `640x720` Liquid-Glass Card inside the monitor's Fog Veil.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardAnchor {
    #[default]
    MiddleLeft,
    Center,
    BottomLeft,
    BottomCenter,
}

impl CardAnchor {
    /// Cycles to the next anchor preset for the in-menu switcher pill.
    pub fn next(self) -> Self {
        match self {
            Self::MiddleLeft => Self::Center,
            Self::Center => Self::BottomLeft,
            Self::BottomLeft => Self::BottomCenter,
            Self::BottomCenter => Self::MiddleLeft,
        }
    }

    /// Human-readable label shown in the Start menu anchor switcher pill.
    pub fn label(self) -> &'static str {
        match self {
            Self::MiddleLeft => "Middle Left",
            Self::Center => "Center",
            Self::BottomLeft => "Bottom Left",
            Self::BottomCenter => "Bottom Center",
        }
    }

    /// Serialized identifier used in `config.toml`.
    pub fn as_key(self) -> &'static str {
        match self {
            Self::MiddleLeft => "middle-left",
            Self::Center => "center",
            Self::BottomLeft => "bottom-left",
            Self::BottomCenter => "bottom-center",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "middle-left" | "middle_left" | "left" => Some(Self::MiddleLeft),
            "center" | "middle" => Some(Self::Center),
            "bottom-left" | "bottom_left" => Some(Self::BottomLeft),
            "bottom-center" | "bottom_center" | "bottom" => Some(Self::BottomCenter),
            _ => None,
        }
    }

    /// Returns the top-left `(x, y)` coordinates of the `640x720` card in logical pixels within a
    /// monitor of `(monitor_width, monitor_height)` logical pixels.
    pub fn card_origin(self, monitor_width: f32, monitor_height: f32, edge_margin: f32) -> (f32, f32) {
        let max_x: f32 = (monitor_width - CARD_WIDTH).max(0.0);
        let max_y: f32 = (monitor_height - CARD_HEIGHT).max(0.0);
        let margin: f32 = edge_margin.max(0.0);

        let x: f32 = match self {
            Self::MiddleLeft | Self::BottomLeft => margin.min(max_x),
            Self::Center | Self::BottomCenter => (max_x * 0.5).round(),
        };

        let y: f32 = match self {
            Self::MiddleLeft | Self::Center => (max_y * 0.5).round(),
            Self::BottomLeft | Self::BottomCenter => (max_y - margin).max(0.0),
        };

        (x, y)
    }
}

/// User configuration loaded from `config.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub card_anchor: CardAnchor,
    pub edge_margin: f32,
    pub use_win_key: bool,
    pub use_taskbar_hook: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            card_anchor: CardAnchor::MiddleLeft,
            edge_margin: DEFAULT_EDGE_MARGIN,
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

    /// Saves the current configuration to `config.toml`.
    pub fn save(self) -> Result<()> {
        let path: PathBuf = config_file_path().context("failed to determine the configuration directory")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("failed to create the configuration directory")?;
        }

        fs::write(&path, self.to_toml()).context("failed to write config.toml")
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
                "use_win_key" => match value.to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => config.use_win_key = true,
                    "false" | "0" | "no" => config.use_win_key = false,
                    _ => {}
                },
                "use_taskbar_hook" => match value.to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => config.use_taskbar_hook = true,
                    "false" | "0" | "no" => config.use_taskbar_hook = false,
                    _ => {}
                },
                _ => {}
            }
        }

        config
    }

    pub fn to_toml(self) -> String {
        format!(
            "# Rusty Start configuration\n\
             # Options: \"middle-left\" (default), \"center\", \"bottom-left\", \"bottom-center\"\n\
             card_anchor = \"{}\"\n\
             edge_margin = {}\n\
             use_win_key = {}\n\
             use_taskbar_hook = {}\n",
            self.card_anchor.as_key(),
            self.edge_margin as u32,
            self.use_win_key,
            self.use_taskbar_hook,
        )
    }
}

/// Directory holding `config.toml` and `usage.tsv`: `%APPDATA%\RustyStart` on Windows,
/// `$XDG_CONFIG_HOME/rusty-start` (or `~/.config/rusty-start`) elsewhere.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(appdata) = env::var_os("APPDATA") {
        return Some(PathBuf::from(appdata).join("RustyStart"));
    }
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("rusty-start"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("rusty-start"))
}

fn config_file_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_anchor_is_middle_left() {
        let config = Config::default();

        assert_eq!(config.card_anchor, CardAnchor::MiddleLeft);
        assert_eq!(config.edge_margin, 48.0);
        assert!(config.use_win_key);
        assert!(config.use_taskbar_hook);
    }

    #[test]
    fn computes_middle_left_and_other_origins_across_resolutions() {
        assert_eq!(CardAnchor::MiddleLeft.card_origin(1920.0, 1080.0, 48.0), (48.0, 180.0));
        assert_eq!(CardAnchor::Center.card_origin(1920.0, 1080.0, 48.0), (640.0, 180.0));
        assert_eq!(CardAnchor::BottomLeft.card_origin(2560.0, 1440.0, 48.0), (48.0, 672.0));
        assert_eq!(CardAnchor::BottomCenter.card_origin(2560.0, 1440.0, 48.0), (960.0, 672.0));
    }

    #[test]
    fn cycles_through_all_anchors_in_order() {
        let start = CardAnchor::MiddleLeft;

        assert_eq!(start.next(), CardAnchor::Center);
        assert_eq!(start.next().next(), CardAnchor::BottomLeft);
        assert_eq!(start.next().next().next(), CardAnchor::BottomCenter);
        assert_eq!(start.next().next().next().next(), CardAnchor::MiddleLeft);
    }

    #[test]
    fn round_trips_toml_serialization() {
        let original = Config {
            card_anchor: CardAnchor::BottomLeft,
            edge_margin: 64.0,
            use_win_key: false,
            use_taskbar_hook: false,
        };
        let parsed = Config::parse_toml(&original.to_toml());

        assert_eq!(parsed, original);
    }
}
