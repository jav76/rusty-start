//! Values `rusty-start setup` writes, kept apart from the I/O so they can be tested.

use std::path::Path;

use rusty_start_protocol::APP_ID;

use super::ItemState;

/// Key that marks files `rusty-start setup` wrote, so `--undo` never removes a user's own file.
pub const SETUP_MARKER: &str = "X-RustyStart-Setup=true";

/// Qt key code of the Meta (Windows) key on its own, as kglobalaccel stores modifier-only
/// shortcuts.
pub const KEY_META: i32 = 0x0100_0022;

/// One kglobalaccel key sequence: up to four Qt key codes, zero-padded.
pub type KeySequence = Vec<i32>;

/// The sequence for Meta on its own.
pub fn meta_sequence() -> KeySequence {
    vec![KEY_META, 0, 0, 0]
}

pub fn has_meta(keys: &[KeySequence]) -> bool {
    keys.iter().any(|sequence| is_meta(sequence))
}

/// `keys` without Meta on its own, keeping every other shortcut (Alt+F1 for the launcher).
pub fn without_meta(keys: &[KeySequence]) -> Vec<KeySequence> {
    keys.iter().filter(|sequence| !is_meta(sequence)).cloned().collect()
}

fn is_meta(sequence: &[i32]) -> bool {
    sequence.first() == Some(&KEY_META) && sequence[1..].iter().all(|key| *key == 0)
}

/// Saves the launcher's shortcuts before setup took Meta from it: one sequence per line, its key
/// codes separated by commas.
pub fn serialize_keys(keys: &[KeySequence]) -> String {
    keys.iter()
        .map(|sequence| sequence.iter().map(i32::to_string).collect::<Vec<String>>().join(","))
        .map(|line| line + "\n")
        .collect()
}

pub fn parse_keys(contents: &str) -> Option<Vec<KeySequence>> {
    contents
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(',').map(|key| key.trim().parse::<i32>().ok()).collect::<Option<KeySequence>>())
        .collect()
}

/// `kwinrulesrc [General] rules` with `rule` appended once.
pub fn with_rule(rules: &str, rule: &str) -> String {
    let mut names: Vec<&str> = split_rules(rules).collect();
    if !names.contains(&rule) {
        names.push(rule);
    }
    names.join(",")
}

/// `kwinrulesrc [General] rules` without `rule`.
pub fn without_rule(rules: &str, rule: &str) -> String {
    split_rules(rules).filter(|name| *name != rule).collect::<Vec<&str>>().join(",")
}

pub fn rule_count(rules: &str) -> usize {
    split_rules(rules).count()
}

fn split_rules(rules: &str) -> impl Iterator<Item = &str> {
    rules.split(',').map(str::trim).filter(|name| !name.is_empty())
}

/// Keys of the KWin window rule for the overlay: kept out of the taskbar, pager and task switcher,
/// and focused without focus-stealing prevention. Rule `2` is "Force", match `1` is "Exact".
pub fn window_rule_keys() -> Vec<(&'static str, String)> {
    vec![
        ("Description", "Rusty Start overlay".to_owned()),
        ("wmclass", APP_ID.to_owned()),
        ("wmclassmatch", "1".to_owned()),
        ("wmclasscomplete", "false".to_owned()),
        ("skiptaskbar", "true".to_owned()),
        ("skiptaskbarrule", "2".to_owned()),
        ("skippager", "true".to_owned()),
        ("skippagerrule", "2".to_owned()),
        ("skipswitcher", "true".to_owned()),
        ("skipswitcherrule", "2".to_owned()),
        ("fsplevel", "0".to_owned()),
        ("fsplevelrule", "2".to_owned()),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// `~/.config/autostart`: starts the host with the session.
    Autostart,
    /// `~/.local/share/applications`: lets KWin authorize a development build's screenshots.
    Development,
}

/// Desktop entry for `executable`. Hidden from launchers, including Rusty Start itself.
pub fn desktop_entry(executable: &Path, kind: EntryKind) -> String {
    let name: &str = match kind {
        EntryKind::Autostart => "Rusty Start",
        EntryKind::Development => "Rusty Start (development build)",
    };
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Exec={}\n\
         NoDisplay=true\n\
         X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2\n\
         {SETUP_MARKER}\n",
        quote_exec(&executable.to_string_lossy())
    )
}

/// Audits a desktop entry setup writes, given its current contents (`None` when it is missing) and
/// the contents setup would write now.
pub fn entry_state(existing: Option<&str>, expected: &str) -> (ItemState, &'static str) {
    match existing {
        None => (ItemState::Missing, "not installed"),
        Some(contents) if contents == expected => (ItemState::Installed, "installed"),
        Some(contents) if contents.lines().any(|line| line == SETUP_MARKER) => {
            (ItemState::Conflict, "points at another build; Install updates it")
        }
        Some(_) => (ItemState::Conflict, "a file setup did not write is in the way"),
    }
}

/// Quotes a program path for `Exec` when it contains characters the specification reserves.
fn quote_exec(program: &str) -> String {
    let reserved = |character: char| character.is_whitespace() || "\"'\\><~|&;$*?#()`".contains(character);
    if !program.contains(reserved) {
        return program.to_owned();
    }
    let mut quoted = String::with_capacity(program.len() + 2);
    quoted.push('"');
    for character in program.chars() {
        if matches!(character, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    // The Exec value is itself a string value, whose escaping doubles every backslash.
    quoted.replace('\\', "\\\\")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const ALT_F1: i32 = 0x0900_0030;

    #[test]
    fn taking_meta_keeps_the_launchers_other_shortcuts() {
        let launcher: Vec<KeySequence> = vec![meta_sequence(), vec![ALT_F1, 0, 0, 0]];

        assert!(has_meta(&launcher));
        assert_eq!(without_meta(&launcher), [vec![ALT_F1, 0, 0, 0]]);
        assert!(!has_meta(&without_meta(&launcher)));
        // A chord that starts with Meta is not the Meta key on its own.
        assert!(!has_meta(&[vec![KEY_META, ALT_F1, 0, 0]]));
    }

    #[test]
    fn saved_keys_round_trip() {
        let launcher: Vec<KeySequence> = vec![meta_sequence(), vec![ALT_F1, 0, 0, 0]];

        assert_eq!(parse_keys(&serialize_keys(&launcher)), Some(launcher));
        assert_eq!(parse_keys(""), Some(Vec::new()));
        assert_eq!(parse_keys("1,x"), None);
    }

    #[test]
    fn rule_lists_add_once_and_remove_cleanly() {
        let existing = "b1fe5fc2-f11a-415a-9089-4813121bb452";

        assert_eq!(with_rule(existing, APP_ID), format!("{existing},{APP_ID}"));
        assert_eq!(with_rule(&with_rule(existing, APP_ID), APP_ID), format!("{existing},{APP_ID}"));
        assert_eq!(with_rule("", APP_ID), APP_ID);
        assert_eq!(without_rule(&format!("{existing},{APP_ID}"), APP_ID), existing);
        assert_eq!(without_rule(APP_ID, APP_ID), "");
        assert_eq!(rule_count(&format!("{existing}, {APP_ID},")), 2);
    }

    #[test]
    fn window_rule_matches_the_app_id_exactly_and_forces_its_settings() {
        let keys = window_rule_keys();
        let value = |name: &str| keys.iter().find(|(key, _)| *key == name).map(|(_, value)| value.as_str());

        assert_eq!((value("wmclass"), value("wmclassmatch")), (Some(APP_ID), Some("1")));
        for forced in ["skiptaskbarrule", "skippagerrule", "skipswitcherrule", "fsplevelrule"] {
            assert_eq!(value(forced), Some("2"), "{forced}");
        }
    }

    #[test]
    fn desktop_entries_are_hidden_and_marked() {
        let entry = desktop_entry(&PathBuf::from("/usr/bin/rusty-start"), EntryKind::Autostart);

        assert!(entry.contains("Exec=/usr/bin/rusty-start\n"));
        assert!(entry.contains("NoDisplay=true\n"));
        assert!(entry.contains(SETUP_MARKER));
    }

    #[test]
    fn entries_are_audited_against_what_setup_would_write() {
        let expected = desktop_entry(&PathBuf::from("/usr/bin/rusty-start"), EntryKind::Autostart);
        let other_build = desktop_entry(&PathBuf::from("/home/me/rusty-start"), EntryKind::Autostart);

        assert_eq!(entry_state(None, &expected).0, ItemState::Missing);
        assert_eq!(entry_state(Some(&expected), &expected).0, ItemState::Installed);
        assert_eq!(entry_state(Some(&other_build), &expected).0, ItemState::Conflict);
        assert_eq!(entry_state(Some("[Desktop Entry]\nExec=mine\n"), &expected).0, ItemState::Conflict);
    }

    #[test]
    fn exec_paths_with_reserved_characters_are_quoted() {
        let entry = desktop_entry(&PathBuf::from("/home/me/My Builds/rusty-start"), EntryKind::Development);
        assert!(entry.contains("Exec=\"/home/me/My Builds/rusty-start\"\n"));

        assert_eq!(quote_exec("/opt/a$b"), "\"/opt/a\\\\$b\"");
    }
}
