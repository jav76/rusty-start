//! Windows launch IDs, the parsing names of items in the shell's Applications folder: an
//! AppUserModelID for packaged apps and programs that set one, otherwise the program's file path.

use super::AppAction;

/// The file a launch ID names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellFile<'a> {
    /// A path below a known folder, such as `{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\cmd.exe` for
    /// `cmd.exe` in System32. `folder_id` is the folder's GUID without braces.
    KnownFolder {
        folder_id: &'a str,
        relative: &'a str,
    },
    Absolute(&'a str),
}

/// The file `launch_id` names, or `None` for an AppUserModelID.
pub fn file(launch_id: &str) -> Option<ShellFile<'_>> {
    if let Some(rest) = launch_id.strip_prefix('{')
        && let Some((folder_id, relative)) = rest.split_once("}\\")
        && !folder_id.is_empty()
        && !relative.is_empty()
    {
        return Some(ShellFile::KnownFolder { folder_id, relative });
    }

    let bytes: &[u8] = launch_id.as_bytes();
    let is_drive_path: bool =
        bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    is_drive_path.then_some(ShellFile::Absolute(launch_id))
}

/// Program name of a launch ID that is a path to an `.exe`, such as `msedge` for
/// `{6D809377-...}\Microsoft\Edge\Application\msedge.exe`, so apps are found by the name
/// typed in Run dialogs. Packaged apps (AppUserModelIDs) have none.
pub fn exe_keyword(launch_id: &str) -> Option<String> {
    let file_name: &str = launch_id.rsplit(['\\', '/']).next()?;
    let (stem, extension) = file_name.rsplit_once('.')?;

    (extension.eq_ignore_ascii_case("exe") && !stem.is_empty()).then(|| stem.to_owned())
}

/// App Action Flyout entries for `launch_id`. Only programs listed by file path can run as
/// administrator or show their location: packaged apps and other AppUserModelIDs name no file.
/// Every app can be uninstalled from Settings.
pub fn actions(launch_id: &str) -> Vec<AppAction> {
    let is_file: bool = file(launch_id).is_some();
    let mut actions: Vec<AppAction> = Vec::with_capacity(3);
    if is_file && exe_keyword(launch_id).is_some() {
        actions.push(AppAction::RunAsAdministrator);
    }
    if is_file {
        actions.push(AppAction::OpenFileLocation);
    }
    actions.push(AppAction::Uninstall);
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\cmd.exe";
    const CALCULATOR: &str = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";

    #[test]
    fn exe_keyword_is_the_program_file_name_without_extension() {
        assert_eq!(
            exe_keyword("{6D809377-6AF0-444B-8957-A3773F02200E}\\Tools\\mspaint.EXE").as_deref(),
            Some("mspaint")
        );
        assert_eq!(exe_keyword("C:/Apps/my.tool.exe").as_deref(), Some("my.tool"));
    }

    #[test]
    fn packaged_apps_and_other_files_have_no_exe_keyword() {
        assert_eq!(exe_keyword(CALCULATOR), None);
        assert_eq!(exe_keyword("Microsoft.Office.WINWORD.EXE.15"), None);
        assert_eq!(exe_keyword("{GUID}\\Docs\\readme.txt"), None);
        assert_eq!(exe_keyword("{GUID}\\.exe"), None);
    }

    #[test]
    fn known_folder_and_drive_paths_name_files() {
        assert_eq!(
            file(CMD),
            Some(ShellFile::KnownFolder { folder_id: "1AC14E77-02E7-4E5D-B744-2EB1AE5198B7", relative: "cmd.exe" })
        );
        assert_eq!(file("D:\\Tools\\app.exe"), Some(ShellFile::Absolute("D:\\Tools\\app.exe")));
    }

    #[test]
    fn app_user_model_ids_name_no_file() {
        assert_eq!(file(CALCULATOR), None);
        assert_eq!(file("MSEdge"), None);
        assert_eq!(file("{}\\cmd.exe"), None);
        assert_eq!(file("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\"), None);
    }

    #[test]
    fn programs_can_run_as_administrator_and_show_their_location() {
        assert_eq!(actions(CMD), [AppAction::RunAsAdministrator, AppAction::OpenFileLocation, AppAction::Uninstall]);
    }

    #[test]
    fn packaged_apps_get_no_run_as_administrator() {
        assert_eq!(actions(CALCULATOR), [AppAction::Uninstall]);
        assert_eq!(actions("MSEdge"), [AppAction::Uninstall]);
    }

    #[test]
    fn files_other_than_programs_only_show_their_location() {
        assert_eq!(
            actions("{F38BF404-1D43-42F2-9305-67DE0B28FC23}\\Help\\guide.chm"),
            [AppAction::OpenFileLocation, AppAction::Uninstall]
        );
    }
}
