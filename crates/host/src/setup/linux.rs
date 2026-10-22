//! Linux setup. On KDE Plasma the Meta key moves through kglobalaccel, which owns it: plasmashell's
//! Application Launcher keeps its other shortcuts (Alt+F1) and Meta goes to the host's `toggle`
//! action. The KWin window rule is written with `kwriteconfig6`, the supported way to edit KConfig
//! files. Other desktops get instructions instead. Every change is
//! reported, running setup twice changes nothing, and undo only reverses what setup itself did.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use rusty_start_protocol::{APP_ID, GLOBAL_SHORTCUT_ACTION_ID};
use zbus::blocking::Proxy;

use super::values::{self, EntryKind, KeySequence, SETUP_MARKER};
use super::{ItemState, MetaKeyState, Options, SetupItem};
use crate::{config, dbus};

/// On Linux the Meta key belongs to the desktop, so the Settings Page's switch edits the KDE
/// global shortcut through setup instead of `use_win_key`.
pub const OWNS_META_KEY: bool = true;

const KWIN_RULES: &str = "kwinrulesrc";
/// Group of the KWin window rule. Any name works as long as `[General] rules` lists it.
const RULE_GROUP: &str = APP_ID;
/// plasmashell's kglobalaccel action that owns Meta by default.
const LAUNCHER_ACTION_ID: [&str; 4] =
    ["plasmashell", "activate application launcher", "Plasma", "Activate Application Launcher"];
/// The launcher's shortcuts before setup took Meta from it, restored by undo.
const PREVIOUS_LAUNCHER_FILE: &str = "setup-launcher-keys";
const MANUAL_TRIGGER_INSTRUCTIONS: &str = "This desktop has no Meta key binding Rusty Start can set. Bind a keyboard \
     shortcut to run `rusty-start` in your desktop's keyboard settings: a second launch toggles the menu.";

pub fn apply(options: Options, changes: &mut Vec<String>) -> Result<()> {
    let executable: PathBuf = env::current_exe().context("failed to find the running executable")?;

    if is_plasma() {
        bind_meta_key(changes)?;
        add_window_rule(changes)?;
        reconfigure_kwin();
    } else {
        changes.push(MANUAL_TRIGGER_INSTRUCTIONS.to_owned());
    }

    write_marked_file(
        &autostart_entry_path()?,
        &values::desktop_entry(&executable, EntryKind::Autostart),
        "autostart",
        changes,
    )?;
    if options.dev {
        write_marked_file(
            &development_entry_path()?,
            &values::desktop_entry(&executable, EntryKind::Development),
            "the development desktop entry, so KWin authorizes desktop snapshots for this build",
            changes,
        )?;
    }

    if !options.from_host {
        changes.push("Done. Start the host with `rusty-start`; it also starts with your next session.".to_owned());
    }
    Ok(())
}

pub fn undo(options: Options, changes: &mut Vec<String>) -> Result<()> {
    if is_plasma() {
        restore_meta_key(options, changes)?;
        remove_window_rule(changes)?;
        reconfigure_kwin();
    }
    remove_marked_file(&autostart_entry_path()?, "autostart", changes)?;
    remove_marked_file(&development_entry_path()?, "the development desktop entry", changes)?;
    if !options.from_host {
        changes.push("Done. Stop a running host with `pkill -x rusty-start`.".to_owned());
    }
    Ok(())
}

/// The Setup Audit: how each piece setup manages stands right now.
pub fn audit() -> Result<Vec<SetupItem>> {
    let executable: PathBuf = env::current_exe().context("failed to find the running executable")?;
    let mut items: Vec<SetupItem> = Vec::new();

    if is_plasma() {
        let proxy: Proxy<'static> = kglobalaccel()?;
        items.push(if values::has_meta(&shortcut_keys(&proxy, GLOBAL_SHORTCUT_ACTION_ID)?) {
            SetupItem::new("Meta key", ItemState::Installed, "opens Rusty Start")
        } else if values::has_meta(&shortcut_keys(&proxy, LAUNCHER_ACTION_ID)?) {
            SetupItem::new("Meta key", ItemState::Missing, "opens the Application Launcher")
        } else {
            SetupItem::new("Meta key", ItemState::Missing, "not bound")
        });

        let rules: String = read_kconfig(KWIN_RULES, "General", "rules")?;
        items.push(if values::with_rule(&rules, RULE_GROUP) == rules {
            SetupItem::new("KWin window rule", ItemState::Installed, "installed")
        } else {
            SetupItem::new("KWin window rule", ItemState::Missing, "not installed")
        });
    } else {
        items.push(SetupItem::new("Meta key", ItemState::Unavailable, "bind a desktop shortcut to `rusty-start`"));
    }

    items.push(entry_item("Autostart", &autostart_entry_path()?, &executable, EntryKind::Autostart));
    let development_entry: PathBuf = development_entry_path()?;
    if development_entry.exists() {
        items.push(entry_item("Development entry", &development_entry, &executable, EntryKind::Development));
    }
    Ok(items)
}

fn entry_item(label: &'static str, path: &Path, executable: &Path, kind: EntryKind) -> SetupItem {
    let existing: Option<String> = fs::read_to_string(path).ok();
    let (state, detail) = values::entry_state(existing.as_deref(), &values::desktop_entry(executable, kind));
    SetupItem::new(label, state, detail)
}

/// What the Settings Page's Meta key switch shows.
pub fn meta_key() -> Result<MetaKeyState> {
    if !is_plasma() {
        return Ok(MetaKeyState { enabled: false, available: false, detail: MANUAL_TRIGGER_INSTRUCTIONS.to_owned() });
    }
    let proxy: Proxy<'static> = kglobalaccel()?;
    let enabled: bool = values::has_meta(&shortcut_keys(&proxy, GLOBAL_SHORTCUT_ACTION_ID)?);
    let detail: &str =
        if enabled { "" } else { "Meta opens the Application Launcher; Rusty Start keeps its other triggers" };
    Ok(MetaKeyState { enabled, available: true, detail: detail.to_owned() })
}

/// Binds Meta to the host's KDE global shortcut, or gives it back to the Application Launcher.
/// Runs from the host, so its action stays registered either way.
pub fn set_meta_key(enabled: bool, changes: &mut Vec<String>) -> Result<()> {
    if !is_plasma() {
        bail!("this desktop has no Meta key binding Rusty Start can set");
    }
    if enabled {
        bind_meta_key(changes)
    } else {
        restore_meta_key(Options { from_host: true, ..Options::default() }, changes)
    }
}

fn is_plasma() -> bool {
    let on_plasma: bool = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .any(|desktop| desktop.eq_ignore_ascii_case("KDE"));
    on_plasma && kglobalaccel().is_ok_and(|proxy| shortcut_keys(&proxy, LAUNCHER_ACTION_ID).is_ok())
}

fn bind_meta_key(changes: &mut Vec<String>) -> Result<()> {
    let proxy: Proxy<'static> = kglobalaccel()?;
    proxy
        .call_method("doRegister", &(GLOBAL_SHORTCUT_ACTION_ID.to_vec(),))
        .context("failed to register the shortcut")?;
    if values::has_meta(&shortcut_keys(&proxy, GLOBAL_SHORTCUT_ACTION_ID)?) {
        changes.push("Meta key: already opens Rusty Start".to_owned());
        return Ok(());
    }

    let launcher: Vec<KeySequence> = shortcut_keys(&proxy, LAUNCHER_ACTION_ID)?;
    if values::has_meta(&launcher) {
        let previous_file: PathBuf = state_path(PREVIOUS_LAUNCHER_FILE)?;
        if !previous_file.exists() {
            fs::create_dir_all(previous_file.parent().context("the state file has a parent")?)
                .context("failed to create the configuration directory")?;
            fs::write(&previous_file, values::serialize_keys(&launcher))
                .context("failed to save the Application Launcher's shortcuts")?;
        }
        changes.push("Meta key: taken from the Application Launcher, which keeps its other shortcuts".to_owned());
        set_shortcut_keys(&proxy, LAUNCHER_ACTION_ID, &values::without_meta(&launcher))?;
    }

    changes.push("Meta key: opens Rusty Start (KDE global shortcut \"Toggle the menu\")".to_owned());
    set_shortcut_keys(&proxy, GLOBAL_SHORTCUT_ACTION_ID, &[values::meta_sequence()])
}

fn restore_meta_key(options: Options, changes: &mut Vec<String>) -> Result<()> {
    let proxy: Proxy<'static> = kglobalaccel()?;
    if values::has_meta(&shortcut_keys(&proxy, GLOBAL_SHORTCUT_ACTION_ID)?) {
        changes.push("Meta key: no longer opens Rusty Start".to_owned());
        set_shortcut_keys(&proxy, GLOBAL_SHORTCUT_ACTION_ID, &[])?;
    }
    // The running host listens for this action's presses, so only the command line unregisters it.
    if !options.from_host {
        let _ = proxy.call_method("unRegister", &(GLOBAL_SHORTCUT_ACTION_ID.to_vec(),));
    }

    let previous_file: PathBuf = state_path(PREVIOUS_LAUNCHER_FILE)?;
    if let Ok(contents) = fs::read_to_string(&previous_file) {
        let previous: Vec<KeySequence> =
            values::parse_keys(&contents).context("the saved Application Launcher shortcuts are damaged")?;
        // A Meta binding the user gave the launcher again after setup is theirs to keep.
        if !values::has_meta(&shortcut_keys(&proxy, LAUNCHER_ACTION_ID)?) {
            changes.push("Meta key: opens the Application Launcher again".to_owned());
            set_shortcut_keys(&proxy, LAUNCHER_ACTION_ID, &previous)?;
        }
        fs::remove_file(&previous_file).context("failed to remove the saved Application Launcher shortcuts")?;
    }
    Ok(())
}

fn kglobalaccel() -> Result<Proxy<'static>> {
    Proxy::new(&dbus::session()?, "org.kde.kglobalaccel", "/kglobalaccel", "org.kde.KGlobalAccel")
        .context("failed to reach kglobalaccel")
}

fn shortcut_keys(proxy: &Proxy<'_>, action_id: [&str; 4]) -> Result<Vec<KeySequence>> {
    let keys: Vec<(KeySequence,)> =
        proxy.call("shortcutKeys", &(action_id.to_vec(),)).context("failed to read a KDE global shortcut")?;
    Ok(keys.into_iter().map(|(sequence,)| sequence).collect())
}

fn set_shortcut_keys(proxy: &Proxy<'_>, action_id: [&str; 4], keys: &[KeySequence]) -> Result<()> {
    let keys: Vec<(KeySequence,)> = keys.iter().cloned().map(|sequence| (sequence,)).collect();
    proxy
        .call_method("setForeignShortcutKeys", &(action_id.to_vec(), keys))
        .context("failed to change a KDE global shortcut")?;
    Ok(())
}

fn add_window_rule(changes: &mut Vec<String>) -> Result<()> {
    if values::with_rule(&read_kconfig(KWIN_RULES, "General", "rules")?, RULE_GROUP)
        == read_kconfig(KWIN_RULES, "General", "rules")?
    {
        changes.push("KWin rule: already installed".to_owned());
        return Ok(());
    }
    changes.push(format!(
        "KWin rule: keep the overlay out of the taskbar, pager and task switcher ({KWIN_RULES} [{RULE_GROUP}])"
    ));
    for (key, value) in values::window_rule_keys() {
        write_kconfig(KWIN_RULES, RULE_GROUP, key, &value)?;
    }
    let rules: String = values::with_rule(&read_kconfig(KWIN_RULES, "General", "rules")?, RULE_GROUP);
    write_kconfig(KWIN_RULES, "General", "rules", &rules)?;
    write_kconfig(KWIN_RULES, "General", "count", &values::rule_count(&rules).to_string())
}

fn remove_window_rule(changes: &mut Vec<String>) -> Result<()> {
    let listed: String = read_kconfig(KWIN_RULES, "General", "rules")?;
    let rules: String = values::without_rule(&listed, RULE_GROUP);
    if rules != listed {
        changes.push(format!("KWin rule: removed ({KWIN_RULES} [{RULE_GROUP}])"));
        write_kconfig(KWIN_RULES, "General", "rules", &rules)?;
        write_kconfig(KWIN_RULES, "General", "count", &values::rule_count(&rules).to_string())?;
    }
    for (key, _) in values::window_rule_keys() {
        delete_kconfig(KWIN_RULES, RULE_GROUP, key)?;
    }
    Ok(())
}

/// Asks KWin to reload its configuration so the window rule applies without logging out.
fn reconfigure_kwin() {
    let reloaded = dbus::session().and_then(|connection| {
        connection
            .call_method(Some("org.kde.KWin"), "/KWin", Some("org.kde.KWin"), "reconfigure", &())
            .context("KWin did not reload its configuration")
    });
    if let Err(error) = reloaded {
        eprintln!("{error:#}; the change applies after you log in again");
    }
}

fn write_marked_file(path: &Path, contents: &str, purpose: &str, changes: &mut Vec<String>) -> Result<()> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        changes.push(format!("{}: already installed", path.display()));
        return Ok(());
    }
    if path.exists() && !is_marked(path) {
        bail!("{} exists and was not written by setup; remove it first", path.display());
    }
    changes.push(format!("{}: install {purpose}", path.display()));
    fs::create_dir_all(path.parent().context("the desktop entry has a parent")?)
        .with_context(|| format!("failed to create {}", path.display()))?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn remove_marked_file(path: &Path, purpose: &str, changes: &mut Vec<String>) -> Result<()> {
    if !is_marked(path) {
        return Ok(());
    }
    changes.push(format!("{}: remove {purpose}", path.display()));
    fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
}

fn is_marked(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|contents| contents.lines().any(|line| line == SETUP_MARKER))
}

fn read_kconfig(file: &str, group: &str, key: &str) -> Result<String> {
    let output = Command::new("kreadconfig6")
        .args(["--file", file, "--group", group, "--key", key])
        .output()
        .context("failed to run kreadconfig6")?;
    if !output.status.success() {
        bail!("kreadconfig6 failed for {file} [{group}] {key}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_owned())
}

fn write_kconfig(file: &str, group: &str, key: &str, value: &str) -> Result<()> {
    run_kwriteconfig(&["--file", file, "--group", group, "--key", key, value])
}

fn delete_kconfig(file: &str, group: &str, key: &str) -> Result<()> {
    run_kwriteconfig(&["--file", file, "--group", group, "--key", key, "--delete"])
}

fn run_kwriteconfig(arguments: &[&str]) -> Result<()> {
    let status = Command::new("kwriteconfig6").args(arguments).status().context("failed to run kwriteconfig6")?;
    if !status.success() {
        bail!("kwriteconfig6 {} failed", arguments.join(" "));
    }
    Ok(())
}

fn state_path(file: &str) -> Result<PathBuf> {
    Ok(config::data_dir().context("failed to find the configuration directory")?.join(file))
}

fn autostart_entry_path() -> Result<PathBuf> {
    Ok(xdg_dir("XDG_CONFIG_HOME", ".config")?.join("autostart").join(format!("{APP_ID}.desktop")))
}

fn development_entry_path() -> Result<PathBuf> {
    Ok(xdg_dir("XDG_DATA_HOME", ".local/share")?.join("applications").join(format!("{APP_ID}.desktop")))
}

fn xdg_dir(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(dir) = env::var_os(variable).map(PathBuf::from).filter(|dir| dir.is_absolute()) {
        return Ok(dir);
    }
    let home: PathBuf = env::var_os("HOME").map(PathBuf::from).context("HOME is not set")?;
    Ok(home.join(fallback))
}
