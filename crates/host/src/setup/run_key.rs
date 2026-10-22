//! Values the Windows setup writes to the user's `Run` key, kept apart from the registry calls so
//! they can be tested on any OS.

use std::path::Path;

use super::ItemState;

/// Name of the autostart value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. The
/// name is Rusty Start's own, so setup may overwrite or delete it.
#[cfg_attr(not(backend_win32), expect(dead_code, reason = "only the Windows backend touches the registry"))]
pub const VALUE_NAME: &str = "RustyStart";

/// Command line Windows runs at sign-in: the quoted executable path.
pub fn command(executable: &Path) -> String {
    format!("\"{}\"", executable.display())
}

/// Audits the autostart value, given its current data (`None` when it is missing) and the command
/// setup would write now. Paths compare case-insensitively, like Windows does.
pub fn value_state(existing: Option<&str>, expected: &str) -> (ItemState, &'static str) {
    match existing {
        None => (ItemState::Missing, "not installed"),
        Some(command) if command.trim().eq_ignore_ascii_case(expected) => (ItemState::Installed, "installed"),
        Some(_) => (ItemState::Conflict, "points at another copy of rusty-start; Install updates it"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_command_quotes_paths_with_spaces() {
        let executable = PathBuf::from("C:/Program Files/Rusty Start/rusty-start.exe");

        assert_eq!(command(&executable), "\"C:/Program Files/Rusty Start/rusty-start.exe\"");
    }

    #[test]
    fn the_value_is_audited_against_the_running_executable() {
        let expected = "\"C:\\Tools\\rusty-start.exe\"";

        assert_eq!(value_state(None, expected).0, ItemState::Missing);
        assert_eq!(value_state(Some(expected), expected).0, ItemState::Installed);
        assert_eq!(value_state(Some("\"c:\\tools\\RUSTY-START.exe\""), expected).0, ItemState::Installed);
        assert_eq!(value_state(Some("\"D:\\old\\rusty-start.exe\""), expected).0, ItemState::Conflict);
    }
}
