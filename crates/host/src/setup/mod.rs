//! `rusty-start setup`: one-time desktop integration that the host never does on its own.
//!
//! On Linux it binds the Meta key, adds a KWin window rule, enables autostart and, with `--dev`,
//! authorizes the current executable for KWin screenshots. On Windows it adds an autostart value
//! under the user's `Run` key. `--undo` reverses it and `--check` runs the Setup Audit. The
//! Settings Page calls the same functions from a background thread.

#[cfg(backend_linux)]
mod linux;
#[cfg(backend_linux)]
pub use linux::{OWNS_META_KEY, apply, audit, meta_key, set_meta_key, undo};

#[cfg(backend_win32)]
mod win32;
#[cfg(backend_win32)]
pub use win32::{OWNS_META_KEY, apply, audit, meta_key, set_meta_key, undo};

#[cfg(backend_preview)]
mod preview;
#[cfg(backend_preview)]
pub use preview::{OWNS_META_KEY, apply, audit, meta_key, set_meta_key, undo};

#[cfg(any(backend_win32, test))]
mod run_key;
#[cfg(any(backend_linux, test))]
mod values;

use anyhow::{Result, bail};

const USAGE: &str = "usage: rusty-start setup [--dev] [--undo] [--check]";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Also install a desktop entry for the current executable, so KWin authorizes screenshots
    /// from a development build.
    pub dev: bool,
    /// Called from the running host, which keeps its KDE global shortcut action registered so its
    /// shortcut listener survives an undo.
    pub from_host: bool,
}

/// How one piece of desktop integration stands in the Setup Audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    Installed,
    Missing,
    /// Present but not as setup would leave it, for example pointing at another build.
    Conflict,
    /// Setup cannot manage it on this desktop.
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupItem {
    pub label: &'static str,
    pub state: ItemState,
    pub detail: String,
}

impl SetupItem {
    pub fn new(label: &'static str, state: ItemState, detail: impl Into<String>) -> Self {
        Self { label, state, detail: detail.into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupSummary {
    Installed,
    PartlyInstalled,
    NotInstalled,
    /// Nothing on this desktop can be set up.
    Unavailable,
}

impl SetupSummary {
    pub fn label(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::PartlyInstalled => "Partly installed",
            Self::NotInstalled => "Not installed",
            Self::Unavailable => "Not available on this desktop",
        }
    }
}

/// Summarizes an audit. Items setup cannot manage do not count.
pub fn summarize(items: &[SetupItem]) -> SetupSummary {
    let managed = || items.iter().filter(|item| item.state != ItemState::Unavailable);
    let total: usize = managed().count();
    let installed: usize = managed().filter(|item| item.state == ItemState::Installed).count();

    match (total, installed) {
        (0, _) => SetupSummary::Unavailable,
        (_, 0) => SetupSummary::NotInstalled,
        (total, installed) if total == installed => SetupSummary::Installed,
        _ => SetupSummary::PartlyInstalled,
    }
}

/// Whether undo has anything to reverse: an installed item, or a conflicting one setup wrote.
pub fn can_undo(items: &[SetupItem]) -> bool {
    items.iter().any(|item| matches!(item.state, ItemState::Installed | ItemState::Conflict))
}

/// What the Meta key switch shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaKeyState {
    pub enabled: bool,
    pub available: bool,
    pub detail: String,
}

/// Runs `rusty-start setup` with the arguments that follow `setup`.
#[expect(clippy::print_stdout, reason = "setup is a command-line tool that reports each change")]
pub fn run(arguments: &[String]) -> Result<()> {
    let mut options = Options::default();
    let mut reverse: bool = false;
    let mut check: bool = false;
    for argument in arguments {
        match argument.as_str() {
            "--dev" => options.dev = true,
            "--undo" => reverse = true,
            "--check" => check = true,
            "--help" | "-h" => {
                eprintln!("{USAGE}");
                return Ok(());
            }
            other => bail!("unknown setup option {other:?}\n{USAGE}"),
        }
    }

    if check {
        let items: Vec<SetupItem> = audit()?;
        println!("{}", summarize(&items).label());
        for item in &items {
            println!("  {:<11} {}: {}", format!("{:?}", item.state), item.label, item.detail);
        }
        return Ok(());
    }

    let mut changes: Vec<String> = Vec::new();
    let result: Result<()> = if reverse { undo(options, &mut changes) } else { apply(options, &mut changes) };
    for change in &changes {
        println!("{change}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(states: &[ItemState]) -> Vec<SetupItem> {
        states.iter().map(|&state| SetupItem::new("item", state, "")).collect()
    }

    #[test]
    fn every_managed_item_installed_summarizes_as_installed() {
        let audit = items(&[ItemState::Installed, ItemState::Unavailable, ItemState::Installed]);

        assert_eq!(summarize(&audit), SetupSummary::Installed);
    }

    #[test]
    fn a_conflict_or_missing_item_makes_setup_partly_installed() {
        assert_eq!(summarize(&items(&[ItemState::Installed, ItemState::Conflict])), SetupSummary::PartlyInstalled);
        assert_eq!(summarize(&items(&[ItemState::Missing, ItemState::Installed])), SetupSummary::PartlyInstalled);
    }

    #[test]
    fn nothing_installed_summarizes_as_not_installed() {
        assert_eq!(summarize(&items(&[ItemState::Missing, ItemState::Conflict])), SetupSummary::NotInstalled);
    }

    #[test]
    fn a_desktop_with_nothing_to_manage_is_unavailable() {
        assert_eq!(summarize(&items(&[ItemState::Unavailable])), SetupSummary::Unavailable);
        assert_eq!(summarize(&[]), SetupSummary::Unavailable);
    }

    #[test]
    fn undo_is_offered_only_when_something_is_there_to_reverse() {
        assert!(!can_undo(&items(&[ItemState::Missing, ItemState::Unavailable])));
        assert!(can_undo(&items(&[ItemState::Missing, ItemState::Conflict])));
        assert!(can_undo(&items(&[ItemState::Installed])));
    }
}
