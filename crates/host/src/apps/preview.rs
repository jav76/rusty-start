#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible win32 backend")]

use slint::Rgba8Pixel;

use super::{App, ICON_SIZE, IconPixels};

/// Sample apps with the kind of keywords the platform backends find: program file names on
/// Windows, generic names and keywords on Linux.
const SAMPLE_APPS: [(&str, &[&str]); 12] = [
    ("Calculator", &["calc", "math"]),
    ("Command Prompt", &["cmd", "console"]),
    ("File Explorer", &["explorer", "files"]),
    ("Microsoft Edge", &["msedge", "web browser"]),
    ("Notepad", &["text editor"]),
    ("Paint", &["mspaint", "drawing"]),
    ("PowerShell", &["pwsh", "console"]),
    ("Settings", &["control panel"]),
    ("Snipping Tool", &["screenshot"]),
    ("Task Manager", &["taskmgr", "processes"]),
    ("Terminal", &["wt", "console"]),
    ("Visual Studio Code", &["code", "editor"]),
];

pub fn load() -> anyhow::Result<Vec<App>> {
    Ok(SAMPLE_APPS
        .iter()
        .map(|&(name, keywords)| App {
            name: name.to_owned(),
            launch_id: name.to_owned(),
            keywords: keywords.iter().map(|&keyword| keyword.to_owned()).collect(),
        })
        .collect())
}

/// Solid placeholder tiles with a per-app hue, so the preview shows where icons go.
pub fn load_icons(apps: &[App]) -> anyhow::Result<Vec<(String, IconPixels)>> {
    Ok(apps.iter().map(|app| (app.launch_id.clone(), placeholder_icon(&app.name))).collect())
}

fn placeholder_icon(name: &str) -> IconPixels {
    let hue: u32 = name.bytes().fold(0_u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(u32::from(byte)));
    let color =
        Rgba8Pixel { r: 64 + (hue % 160) as u8, g: 64 + (hue / 7 % 160) as u8, b: 160 + (hue / 3 % 96) as u8, a: 255 };

    let mut icon = IconPixels::new(ICON_SIZE, ICON_SIZE);
    icon.make_mut_slice().fill(color);
    icon
}

pub fn launch(app: &App) {
    eprintln!("preview mode: would launch {}", app.name);
}

/// Placeholder guard so the host's startup code is identical on every platform.
pub struct AppWatcher;

/// The sample list never changes, so there is nothing to watch.
pub fn watch(_on_change: impl Fn(Vec<App>) + Send + 'static) -> anyhow::Result<AppWatcher> {
    Ok(AppWatcher)
}
