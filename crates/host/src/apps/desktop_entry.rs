//! The subset of the freedesktop Desktop Entry specification the menu needs: the `[Desktop Entry]`
//! group, localized names, visibility rules and desktop file IDs.

use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "mirrors the boolean keys of the specification")]
pub struct DesktopEntry {
    pub name: String,
    pub exec: Option<String>,
    pub icon: Option<String>,
    /// Working directory for the launched program.
    pub path: Option<String>,
    pub try_exec: Option<String>,
    pub terminal: bool,
    pub dbus_activatable: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub is_application: bool,
}

impl DesktopEntry {
    /// Parses the `[Desktop Entry]` group. `locales` lists locale names in preference order (see
    /// `locale_variants`). Returns `None` when the group or its `Name` is missing.
    pub fn parse(contents: &str, locales: &[String]) -> Option<Self> {
        let mut entry = Self::default();
        let mut in_entry_group: bool = false;
        let mut saw_entry_group: bool = false;
        // Rank of the `Name` kept so far: an index into `locales`, `locales.len()` for the
        // unlocalized key.
        let mut name_rank: Option<usize> = None;

        for raw_line in contents.lines() {
            let line: &str = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_entry_group = line == "[Desktop Entry]";
                saw_entry_group |= in_entry_group;
                continue;
            }
            if !in_entry_group {
                continue;
            }

            let Some((raw_key, raw_value)) = line.split_once('=') else {
                continue;
            };
            let value: String = unescape(raw_value.trim());
            let (key, locale) = split_locale(raw_key.trim());

            match (key, locale) {
                ("Name", None) => {
                    if name_rank.is_none() {
                        entry.name = value;
                        name_rank = Some(locales.len());
                    }
                }
                ("Name", Some(locale)) => {
                    if let Some(rank) = locales.iter().position(|candidate| candidate == locale)
                        && name_rank.is_none_or(|kept| rank < kept)
                    {
                        entry.name = value;
                        name_rank = Some(rank);
                    }
                }
                (_, Some(_)) => {}
                ("Type", None) => entry.is_application = value == "Application",
                ("Exec", None) => entry.exec = Some(value),
                ("Icon", None) => entry.icon = Some(value).filter(|icon| !icon.is_empty()),
                ("Path", None) => entry.path = Some(value).filter(|path| !path.is_empty()),
                ("TryExec", None) => entry.try_exec = Some(value).filter(|program| !program.is_empty()),
                ("Terminal", None) => entry.terminal = value == "true",
                ("DBusActivatable", None) => entry.dbus_activatable = value == "true",
                ("NoDisplay", None) => entry.no_display = value == "true",
                ("Hidden", None) => entry.hidden = value == "true",
                ("OnlyShowIn", None) => entry.only_show_in = split_list(&value),
                ("NotShowIn", None) => entry.not_show_in = split_list(&value),
                _ => {}
            }
        }

        (saw_entry_group && !entry.name.is_empty()).then_some(entry)
    }

    /// Whether the menu lists this entry on a session whose `XDG_CURRENT_DESKTOP` names
    /// `desktops`. `program_exists` resolves `TryExec`.
    pub fn is_visible(&self, desktops: &[String], program_exists: impl Fn(&str) -> bool) -> bool {
        let names_current = |list: &[String]| {
            list.iter().any(|listed| desktops.iter().any(|desktop| listed.eq_ignore_ascii_case(desktop)))
        };

        self.is_application
            && !self.no_display
            && !self.hidden
            && (self.exec.is_some() || self.dbus_activatable)
            && (self.only_show_in.is_empty() || names_current(&self.only_show_in))
            && !names_current(&self.not_show_in)
            && self.try_exec.as_deref().is_none_or(program_exists)
    }
}

/// Locale names to try for localized keys, most specific first, from a POSIX locale such as
/// `de_DE.UTF-8@euro`. `C` and `POSIX` have none.
pub fn locale_variants(locale: &str) -> Vec<String> {
    let (without_modifier, modifier) = match locale.split_once('@') {
        Some((head, modifier)) => (head, Some(modifier)),
        None => (locale, None),
    };
    let without_encoding: &str = without_modifier.split('.').next().unwrap_or_default();
    if without_encoding.is_empty() || without_encoding == "C" || without_encoding == "POSIX" {
        return Vec::new();
    }
    let (lang, country) = match without_encoding.split_once('_') {
        Some((lang, country)) => (lang, Some(country)),
        None => (without_encoding, None),
    };

    let mut variants: Vec<String> = Vec::with_capacity(4);
    if let (Some(country), Some(modifier)) = (country, modifier) {
        variants.push(format!("{lang}_{country}@{modifier}"));
    }
    if let Some(country) = country {
        variants.push(format!("{lang}_{country}"));
    }
    if let Some(modifier) = modifier {
        variants.push(format!("{lang}@{modifier}"));
    }
    variants.push(lang.to_owned());
    variants
}

/// Desktop file ID of `file` found under the applications directory `applications_dir`: the
/// relative path with `/` replaced by `-`, for example `kde4/konsole.desktop` -> `kde4-konsole.desktop`.
pub fn desktop_file_id(applications_dir: &Path, file: &Path) -> Option<String> {
    let relative: &Path = file.strip_prefix(applications_dir).ok()?;
    let parts: Vec<&str> = relative.components().map(|part| part.as_os_str().to_str()).collect::<Option<_>>()?;
    let id: String = parts.join("-");
    id.ends_with(".desktop").then_some(id)
}

fn split_locale(key: &str) -> (&str, Option<&str>) {
    match key.split_once('[') {
        Some((key, rest)) => (key.trim_end(), rest.strip_suffix(']')),
        None => (key, None),
    }
}

fn split_list(value: &str) -> Vec<String> {
    value.split(';').map(str::trim).filter(|item| !item.is_empty()).map(str::to_owned).collect()
}

/// Resolves the escapes the specification allows in string values.
fn unescape(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            unescaped.push(character);
            continue;
        }
        match chars.next() {
            Some('s') => unescaped.push(' '),
            Some('n') => unescaped.push('\n'),
            Some('t') => unescaped.push('\t'),
            Some('r') => unescaped.push('\r'),
            Some('\\') => unescaped.push('\\'),
            // Other escapes (`\;` in lists) belong to the value's own syntax; keep them as written.
            Some(other) => {
                unescaped.push('\\');
                unescaped.push(other);
            }
            None => unescaped.push('\\'),
        }
    }
    unescaped
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const KONSOLE: &str = "\
[Desktop Entry]
Type=Application
Name=Konsole
Name[de]=Konsole (de)
Name[de_DE]=Konsole (de_DE)
Exec=konsole
Icon=utilities-terminal
OnlyShowIn=KDE;

[Desktop Action NewWindow]
Name=Open a New Window
Exec=konsole --new-window
";

    fn locales(locale: &str) -> Vec<String> {
        locale_variants(locale)
    }

    #[test]
    fn parses_the_desktop_entry_group_and_ignores_actions() {
        let entry = DesktopEntry::parse(KONSOLE, &[]).expect("valid entry");

        assert_eq!(entry.name, "Konsole");
        assert_eq!(entry.exec.as_deref(), Some("konsole"));
        assert_eq!(entry.icon.as_deref(), Some("utilities-terminal"));
        assert_eq!(entry.only_show_in, ["KDE"]);
        assert!(entry.is_application);
    }

    #[test]
    fn picks_the_most_specific_localized_name() {
        assert_eq!(DesktopEntry::parse(KONSOLE, &locales("de_DE.UTF-8")).expect("valid").name, "Konsole (de_DE)");
        assert_eq!(DesktopEntry::parse(KONSOLE, &locales("de_AT.UTF-8")).expect("valid").name, "Konsole (de)");
        assert_eq!(DesktopEntry::parse(KONSOLE, &locales("fr_FR.UTF-8")).expect("valid").name, "Konsole");
    }

    #[test]
    fn localized_name_before_the_default_still_loses_to_nothing_better() {
        let contents = "[Desktop Entry]\nName[de]=Rechner\nName=Calculator\nType=Application\nExec=calc\n";

        assert_eq!(DesktopEntry::parse(contents, &locales("de_DE")).expect("valid").name, "Rechner");
        assert_eq!(DesktopEntry::parse(contents, &locales("en_US")).expect("valid").name, "Calculator");
    }

    #[test]
    fn locale_variants_follow_the_specification_order() {
        assert_eq!(locale_variants("sr_YU.UTF-8@Latn"), ["sr_YU@Latn", "sr_YU", "sr@Latn", "sr"]);
        assert_eq!(locale_variants("en_US.UTF-8"), ["en_US", "en"]);
        assert!(locale_variants("C.UTF-8").is_empty());
        assert!(locale_variants("").is_empty());
    }

    #[test]
    fn entries_without_a_name_or_group_are_rejected() {
        assert!(DesktopEntry::parse("[Desktop Entry]\nType=Application\nExec=x\n", &[]).is_none());
        assert!(DesktopEntry::parse("Name=Orphan\n", &[]).is_none());
    }

    #[test]
    fn unescapes_string_values() {
        let contents = "[Desktop Entry]\nName=A\\sB\nExec=run \\\\path\nType=Application\n";
        let entry = DesktopEntry::parse(contents, &[]).expect("valid entry");

        assert_eq!(entry.name, "A B");
        assert_eq!(entry.exec.as_deref(), Some("run \\path"));
    }

    #[test]
    fn hidden_no_display_and_non_applications_are_not_visible() {
        let base = DesktopEntry::parse(KONSOLE, &[]).expect("valid entry");
        let kde: Vec<String> = vec!["KDE".to_owned()];

        assert!(base.is_visible(&kde, |_| true));
        assert!(!DesktopEntry { hidden: true, ..base.clone() }.is_visible(&kde, |_| true));
        assert!(!DesktopEntry { no_display: true, ..base.clone() }.is_visible(&kde, |_| true));
        assert!(!DesktopEntry { is_application: false, ..base.clone() }.is_visible(&kde, |_| true));
        assert!(!DesktopEntry { exec: None, ..base }.is_visible(&kde, |_| true));
    }

    #[test]
    fn show_in_lists_match_the_current_desktop() {
        let base = DesktopEntry::parse(KONSOLE, &[]).expect("valid entry");
        let gnome: Vec<String> = vec!["ubuntu".to_owned(), "GNOME".to_owned()];

        assert!(!base.is_visible(&gnome, |_| true));
        assert!(DesktopEntry { only_show_in: Vec::new(), ..base.clone() }.is_visible(&gnome, |_| true));
        assert!(
            !DesktopEntry { only_show_in: Vec::new(), not_show_in: vec!["GNOME".to_owned()], ..base }
                .is_visible(&gnome, |_| true)
        );
    }

    #[test]
    fn missing_try_exec_program_hides_the_entry() {
        let entry =
            DesktopEntry { try_exec: Some("missing".to_owned()), only_show_in: Vec::new(), ..Default::default() };
        let entry = DesktopEntry { name: "X".to_owned(), exec: Some("x".to_owned()), is_application: true, ..entry };

        assert!(!entry.is_visible(&[], |program| program != "missing"));
        assert!(entry.is_visible(&[], |_| true));
    }

    #[test]
    fn desktop_file_ids_join_subdirectories_with_dashes() {
        let dir = PathBuf::from("/usr/share/applications");

        assert_eq!(
            desktop_file_id(&dir, &dir.join("org.kde.konsole.desktop")).as_deref(),
            Some("org.kde.konsole.desktop")
        );
        assert_eq!(desktop_file_id(&dir, &dir.join("kde4/konsole.desktop")).as_deref(), Some("kde4-konsole.desktop"));
        assert_eq!(desktop_file_id(&dir, &dir.join("readme.txt")), None);
        assert_eq!(desktop_file_id(&dir, Path::new("/elsewhere/a.desktop")), None);
    }
}
