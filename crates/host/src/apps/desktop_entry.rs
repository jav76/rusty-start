//! The subset of the freedesktop Desktop Entry specification the menu needs: the `[Desktop Entry]`
//! group and its actions, localized names and keywords, visibility rules and desktop file IDs.

use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "mirrors the boolean keys of the specification")]
pub struct DesktopEntry {
    pub name: String,
    /// What kind of app it is, such as "Web Browser".
    pub generic_name: Option<String>,
    pub keywords: Vec<String>,
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
    /// The app's own actions in the order its `Actions` key lists them.
    pub actions: Vec<DesktopAction>,
}

/// One of an app's own actions, from a `[Desktop Action <id>]` group, such as "New Window".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    /// `None` only for D-Bus activatable apps, which run the action over D-Bus.
    pub exec: Option<String>,
    pub icon: Option<String>,
}

impl DesktopEntry {
    /// Parses the `[Desktop Entry]` group and the action groups it lists. `locales` lists locale
    /// names in preference order (see `locale_variants`). Returns `None` when the group or its
    /// `Name` is missing.
    pub fn parse(contents: &str, locales: &[String]) -> Option<Self> {
        let mut parser = Parser::new(locales);
        for line in contents.lines() {
            parser.read_line(line.trim());
        }
        parser.finish()
    }

    pub fn action(&self, id: &str) -> Option<&DesktopAction> {
        self.actions.iter().find(|action| action.id == id)
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

/// Line-by-line state of `DesktopEntry::parse`.
struct Parser<'a> {
    locales: &'a [String],
    entry: DesktopEntry,
    group: Group,
    saw_entry_group: bool,
    name: Localized,
    generic_name: Localized,
    keywords: Localized,
    /// Action IDs from the `Actions` key. Action groups it does not list are ignored.
    action_ids: Vec<String>,
    action_groups: Vec<ActionGroup>,
}

#[derive(Clone, Copy)]
enum Group {
    Entry,
    /// An index into `Parser::action_groups`.
    Action(usize),
    Other,
}

#[derive(Default)]
struct ActionGroup {
    id: String,
    name: Localized,
    exec: Option<String>,
    icon: Option<String>,
}

impl<'a> Parser<'a> {
    fn new(locales: &'a [String]) -> Self {
        Self {
            locales,
            entry: DesktopEntry::default(),
            group: Group::Other,
            saw_entry_group: false,
            name: Localized::default(),
            generic_name: Localized::default(),
            keywords: Localized::default(),
            action_ids: Vec::new(),
            action_groups: Vec::new(),
        }
    }

    fn read_line(&mut self, line: &str) {
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        if line.starts_with('[') {
            self.start_group(line);
            return;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return;
        };
        let value: String = unescape(raw_value.trim());
        let (key, locale) = split_locale(raw_key.trim());
        match self.group {
            Group::Entry => self.read_entry_key(key, locale, value),
            Group::Action(index) => self.read_action_key(index, key, locale, value),
            Group::Other => {}
        }
    }

    fn start_group(&mut self, header: &str) {
        self.group = if header == "[Desktop Entry]" {
            self.saw_entry_group = true;
            Group::Entry
        } else if let Some(id) = header.strip_prefix("[Desktop Action ").and_then(|rest| rest.strip_suffix(']')) {
            self.action_groups.push(ActionGroup { id: id.to_owned(), ..ActionGroup::default() });
            Group::Action(self.action_groups.len() - 1)
        } else {
            Group::Other
        };
    }

    fn read_entry_key(&mut self, key: &str, locale: Option<&str>, value: String) {
        let entry: &mut DesktopEntry = &mut self.entry;
        match (key, locale) {
            ("Name", _) => self.name.offer(value, locale, self.locales),
            ("GenericName", _) => self.generic_name.offer(value, locale, self.locales),
            ("Keywords", _) => self.keywords.offer(value, locale, self.locales),
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
            ("Actions", None) => self.action_ids = split_list(&value),
            _ => {}
        }
    }

    fn read_action_key(&mut self, index: usize, key: &str, locale: Option<&str>, value: String) {
        let Some(group) = self.action_groups.get_mut(index) else {
            return;
        };
        match (key, locale) {
            ("Name", _) => group.name.offer(value, locale, self.locales),
            ("Exec", None) => group.exec = Some(value),
            ("Icon", None) => group.icon = Some(value).filter(|icon| !icon.is_empty()),
            _ => {}
        }
    }

    fn finish(self) -> Option<DesktopEntry> {
        let mut entry: DesktopEntry = self.entry;
        entry.name = self.name.value.unwrap_or_default();
        entry.generic_name = self.generic_name.value.filter(|generic_name| !generic_name.is_empty());
        entry.keywords = self.keywords.value.as_deref().map(split_list).unwrap_or_default();

        let mut action_groups: Vec<ActionGroup> = self.action_groups;
        entry.actions = self
            .action_ids
            .iter()
            .filter_map(|id| {
                let position: usize = action_groups.iter().position(|group| group.id == *id)?;
                let group: ActionGroup = action_groups.swap_remove(position);
                let name: String = group.name.value.filter(|name| !name.is_empty())?;
                Some(DesktopAction { id: group.id, name, exec: group.exec, icon: group.icon })
            })
            .filter(|action| action.exec.is_some() || entry.dbus_activatable)
            .collect();

        (self.saw_entry_group && !entry.name.is_empty()).then_some(entry)
    }
}

/// The value of a localized key for the most preferred locale seen so far.
#[derive(Default)]
struct Localized {
    value: Option<String>,
    /// An index into the preferred locales, or their count for the unlocalized key.
    rank: Option<usize>,
}

impl Localized {
    fn offer(&mut self, value: String, locale: Option<&str>, locales: &[String]) {
        let rank: Option<usize> = match locale {
            None => Some(locales.len()),
            Some(locale) => locales.iter().position(|candidate| candidate == locale),
        };
        if let Some(rank) = rank
            && self.rank.is_none_or(|kept| rank < kept)
        {
            self.value = Some(value);
            self.rank = Some(rank);
        }
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
GenericName=Terminal
GenericName[de]=Terminalprogramm
Keywords=terminal;console;command line;
Keywords[de]=Kommandozeile;Konsole;
Exec=konsole
Icon=utilities-terminal
OnlyShowIn=KDE;
Actions=NewWindow;NewTab;

[Desktop Action NewTab]
Name=Open a New Tab
Name[de]=Neuer Tab
Exec=konsole --new-tab
Icon=tab-new

[Desktop Action NewWindow]
Name=Open a New Window
Exec=konsole --new-window
";

    fn locales(locale: &str) -> Vec<String> {
        locale_variants(locale)
    }

    #[test]
    fn action_groups_do_not_override_the_desktop_entry_group() {
        let entry = DesktopEntry::parse(KONSOLE, &[]).expect("valid entry");

        assert_eq!(entry.name, "Konsole");
        assert_eq!(entry.exec.as_deref(), Some("konsole"));
        assert_eq!(entry.icon.as_deref(), Some("utilities-terminal"));
        assert_eq!(entry.only_show_in, ["KDE"]);
        assert!(entry.is_application);
    }

    #[test]
    fn parses_actions_in_the_order_the_actions_key_lists_them() {
        let entry = DesktopEntry::parse(KONSOLE, &locales("de_DE.UTF-8")).expect("valid entry");
        let new_window = DesktopAction {
            id: "NewWindow".to_owned(),
            name: "Open a New Window".to_owned(),
            exec: Some("konsole --new-window".to_owned()),
            icon: None,
        };
        let new_tab = DesktopAction {
            id: "NewTab".to_owned(),
            name: "Neuer Tab".to_owned(),
            exec: Some("konsole --new-tab".to_owned()),
            icon: Some("tab-new".to_owned()),
        };

        assert_eq!(entry.actions, [new_window, new_tab.clone()]);
        assert_eq!(entry.action("NewTab"), Some(&new_tab));
        assert_eq!(entry.action("Missing"), None);
    }

    #[test]
    fn skips_unlisted_unnamed_and_unrunnable_actions() {
        let contents = "\
[Desktop Entry]
Type=Application
Name=Editor
Exec=editor
Actions=Unnamed;NoExec;Missing;

[Desktop Action Unnamed]
Exec=editor --unnamed

[Desktop Action NoExec]
Name=No Exec

[Desktop Action Unlisted]
Name=Unlisted
Exec=editor --unlisted
";
        let entry = DesktopEntry::parse(contents, &[]).expect("valid entry");
        let activatable = DesktopEntry::parse(&contents.replace("Exec=editor\n", "DBusActivatable=true\n"), &[])
            .expect("valid entry");

        assert_eq!(entry.actions, []);
        assert_eq!(activatable.actions.iter().map(|action| action.id.as_str()).collect::<Vec<_>>(), ["NoExec"]);
    }

    #[test]
    fn picks_the_most_specific_localized_name() {
        assert_eq!(DesktopEntry::parse(KONSOLE, &locales("de_DE.UTF-8")).expect("valid").name, "Konsole (de_DE)");
        assert_eq!(DesktopEntry::parse(KONSOLE, &locales("de_AT.UTF-8")).expect("valid").name, "Konsole (de)");
        assert_eq!(DesktopEntry::parse(KONSOLE, &locales("fr_FR.UTF-8")).expect("valid").name, "Konsole");
    }

    #[test]
    fn picks_localized_generic_names_and_keywords() {
        let english = DesktopEntry::parse(KONSOLE, &locales("en_US.UTF-8")).expect("valid");
        let german = DesktopEntry::parse(KONSOLE, &locales("de_DE.UTF-8")).expect("valid");

        assert_eq!(english.generic_name.as_deref(), Some("Terminal"));
        assert_eq!(english.keywords, ["terminal", "console", "command line"]);
        assert_eq!(german.generic_name.as_deref(), Some("Terminalprogramm"));
        assert_eq!(german.keywords, ["Kommandozeile", "Konsole"]);
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
        assert_eq!(locale_variants("C.UTF-8"), Vec::<String>::new());
        assert_eq!(locale_variants(""), Vec::<String>::new());
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
