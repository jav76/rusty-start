//! Icon lookup following the freedesktop Icon Theme specification: the current theme, its
//! `Inherits` chain, `hicolor`, then `/usr/share/pixmaps`.
//!
//! Directory listings are read once per lookup session, so resolving hundreds of icons costs one
//! `read_dir` per theme directory instead of a `stat` per candidate file.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Extensions in preference order. PNG decodes faster than SVG renders; XPM is not supported.
const EXTENSIONS: [&str; 2] = ["png", "svg"];
const FALLBACK_THEME: &str = "hicolor";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeKind {
    Fixed,
    Scalable,
    Threshold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeDirectory {
    pub path: String,
    pub size: u32,
    pub scale: u32,
    pub kind: SizeKind,
    pub min_size: u32,
    pub max_size: u32,
    pub threshold: u32,
}

impl ThemeDirectory {
    /// `DirectoryMatchesSize` from the specification.
    pub fn matches(&self, size: u32, scale: u32) -> bool {
        if self.scale != scale {
            return false;
        }
        match self.kind {
            SizeKind::Fixed => self.size == size,
            SizeKind::Scalable => (self.min_size..=self.max_size).contains(&size),
            SizeKind::Threshold => {
                (self.size.saturating_sub(self.threshold)..=self.size + self.threshold).contains(&size)
            }
        }
    }

    /// `DirectorySizeDistance` from the specification, in physical pixels.
    pub fn distance(&self, size: u32, scale: u32) -> u32 {
        let wanted: u32 = size * scale;
        match self.kind {
            SizeKind::Fixed => (self.size * self.scale).abs_diff(wanted),
            SizeKind::Scalable => distance_outside(wanted, self.min_size * self.scale, self.max_size * self.scale),
            SizeKind::Threshold => distance_outside(
                wanted,
                self.size.saturating_sub(self.threshold) * self.scale,
                (self.size + self.threshold) * self.scale,
            ),
        }
    }
}

fn distance_outside(value: u32, low: u32, high: u32) -> u32 {
    if value < low { low - value } else { value.saturating_sub(high) }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeIndex {
    pub inherits: Vec<String>,
    pub directories: Vec<ThemeDirectory>,
}

impl ThemeIndex {
    /// Parses an `index.theme` file.
    pub fn parse(contents: &str) -> Self {
        let groups: HashMap<String, HashMap<String, String>> = parse_ini(contents);
        let Some(theme) = groups.get("Icon Theme") else {
            return Self::default();
        };

        let list = |key: &str| -> Vec<String> {
            theme
                .get(key)
                .map(|value| {
                    value.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_owned).collect()
                })
                .unwrap_or_default()
        };

        let directories: Vec<ThemeDirectory> = list("Directories")
            .into_iter()
            .chain(list("ScaledDirectories"))
            .filter_map(|path| {
                let group: &HashMap<String, String> = groups.get(&path)?;
                let number = |key: &str| group.get(key).and_then(|value| value.parse::<u32>().ok());
                let size: u32 = number("Size")?;
                let kind: SizeKind = match group.get("Type").map(String::as_str) {
                    Some("Fixed") => SizeKind::Fixed,
                    Some("Scalable") => SizeKind::Scalable,
                    _ => SizeKind::Threshold,
                };
                Some(ThemeDirectory {
                    path,
                    size,
                    scale: number("Scale").unwrap_or(1).max(1),
                    kind,
                    min_size: number("MinSize").unwrap_or(size),
                    max_size: number("MaxSize").unwrap_or(size),
                    threshold: number("Threshold").unwrap_or(2),
                })
            })
            .collect();

        Self { inherits: list("Inherits"), directories }
    }
}

/// Reads `key` from `[group]` of an ini-style file such as `kdeglobals` or GTK's `settings.ini`.
pub fn ini_value(contents: &str, group: &str, key: &str) -> Option<String> {
    parse_ini(contents).get(group)?.get(key).cloned()
}

fn parse_ini(contents: &str) -> HashMap<String, HashMap<String, String>> {
    let mut groups: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current: Option<String> = None;

    for raw_line in contents.lines() {
        let line: &str = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            current = Some(name.to_owned());
            continue;
        }
        if let (Some(group), Some((key, value))) = (&current, line.split_once('=')) {
            groups
                .entry(group.clone())
                .or_default()
                .entry(key.trim().to_owned())
                .or_insert_with(|| value.trim().to_owned());
        }
    }
    groups
}

/// Resolves icon names to files. Holds directory listings, so keep one per batch of lookups.
pub struct IconLookup {
    base_dirs: Vec<PathBuf>,
    pixmap_dirs: Vec<PathBuf>,
    /// The current theme, the themes it inherits from (depth first) and `hicolor`.
    themes: Vec<(String, ThemeIndex)>,
    listings: HashMap<PathBuf, HashSet<OsString>>,
}

impl IconLookup {
    /// `base_dirs` are the icon base directories in priority order (`~/.icons`,
    /// `$XDG_DATA_HOME/icons`, `$XDG_DATA_DIRS/icons`).
    pub fn new(theme: &str, base_dirs: Vec<PathBuf>, pixmap_dirs: Vec<PathBuf>) -> Self {
        let mut lookup = Self { base_dirs, pixmap_dirs, themes: Vec::new(), listings: HashMap::new() };
        lookup.load_theme_chain(theme);
        if !lookup.themes.iter().any(|(name, _)| name == FALLBACK_THEME) {
            lookup.load_theme_chain(FALLBACK_THEME);
        }
        lookup
    }

    fn load_theme_chain(&mut self, theme: &str) {
        if self.themes.iter().any(|(name, _)| name == theme) {
            return;
        }
        let Some(index) = self.read_index(theme) else {
            return;
        };
        let inherits: Vec<String> = index.inherits.clone();
        self.themes.push((theme.to_owned(), index));
        for parent in inherits {
            self.load_theme_chain(&parent);
        }
    }

    fn read_index(&self, theme: &str) -> Option<ThemeIndex> {
        self.base_dirs
            .iter()
            .find_map(|base| fs::read_to_string(base.join(theme).join("index.theme")).ok())
            .map(|contents| ThemeIndex::parse(&contents))
    }

    /// Finds the file for `icon` closest to `size` pixels at scale 1. Absolute paths are returned
    /// as they are when the file exists.
    pub fn find(&mut self, icon: &str, size: u32) -> Option<PathBuf> {
        let path = Path::new(icon);
        if path.is_absolute() {
            return path.is_file().then(|| path.to_path_buf());
        }
        // Some entries name the file instead of the icon.
        let name: &str =
            EXTENSIONS.iter().find_map(|extension| icon.strip_suffix(&format!(".{extension}"))).unwrap_or(icon);

        for theme_position in 0..self.themes.len() {
            if let Some(found) = self.find_in_theme(theme_position, name, size) {
                return Some(found);
            }
        }

        let pixmap_dirs: Vec<PathBuf> = self.pixmap_dirs.clone();
        pixmap_dirs.iter().find_map(|dir| self.find_file(dir, name))
    }

    fn find_in_theme(&mut self, theme_position: usize, name: &str, size: u32) -> Option<PathBuf> {
        let (theme, index) = self.themes[theme_position].clone();
        let base_dirs: Vec<PathBuf> = self.base_dirs.clone();

        for directory in index.directories.iter().filter(|directory| directory.matches(size, 1)) {
            for base in &base_dirs {
                if let Some(found) = self.find_file(&base.join(&theme).join(&directory.path), name) {
                    return Some(found);
                }
            }
        }

        let mut closest: Option<(u32, PathBuf)> = None;
        for directory in &index.directories {
            let distance: u32 = directory.distance(size, 1);
            if closest.as_ref().is_some_and(|(best, _)| distance >= *best) {
                continue;
            }
            for base in &base_dirs {
                if let Some(found) = self.find_file(&base.join(&theme).join(&directory.path), name) {
                    closest = Some((distance, found));
                    break;
                }
            }
        }
        closest.map(|(_, found)| found)
    }

    fn find_file(&mut self, dir: &Path, name: &str) -> Option<PathBuf> {
        let listing: &HashSet<OsString> = self.listings.entry(dir.to_path_buf()).or_insert_with(|| {
            fs::read_dir(dir)
                .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.file_name()).collect())
                .unwrap_or_default()
        });
        EXTENSIONS
            .iter()
            .map(|extension| format!("{name}.{extension}"))
            .find_map(|file| listing.contains(std::ffi::OsStr::new(&file)).then(|| dir.join(&file)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    const INDEX: &str = "\
[Icon Theme]
Name=Test
Inherits=parent
Directories=48x48/apps,scalable/apps,16x16/apps
ScaledDirectories=24x24@2x/apps

[48x48/apps]
Size=48
Type=Fixed

[scalable/apps]
Size=48
MinSize=8
MaxSize=512
Type=Scalable

[16x16/apps]
Size=16

[24x24@2x/apps]
Size=24
Scale=2
Type=Fixed
";

    fn directory(index: &ThemeIndex, path: &str) -> ThemeDirectory {
        index.directories.iter().find(|directory| directory.path == path).cloned().expect("directory listed")
    }

    #[test]
    fn parses_directories_with_specification_defaults() {
        let index = ThemeIndex::parse(INDEX);
        let small = directory(&index, "16x16/apps");

        assert_eq!(index.inherits, ["parent"]);
        assert_eq!(index.directories.len(), 4);
        assert_eq!((small.kind, small.scale, small.threshold, small.min_size), (SizeKind::Threshold, 1, 2, 16));
        assert_eq!(directory(&index, "24x24@2x/apps").scale, 2);
    }

    #[test]
    fn size_matching_and_distance_follow_the_specification() {
        let index = ThemeIndex::parse(INDEX);
        let fixed = directory(&index, "48x48/apps");
        let scalable = directory(&index, "scalable/apps");
        let threshold = directory(&index, "16x16/apps");
        let scaled = directory(&index, "24x24@2x/apps");

        assert!(fixed.matches(48, 1) && !fixed.matches(47, 1));
        assert!(scalable.matches(300, 1) && !scalable.matches(600, 1));
        assert!(threshold.matches(18, 1) && !threshold.matches(19, 1));
        assert!(!scaled.matches(24, 1) && scaled.matches(24, 2));
        assert_eq!(fixed.distance(32, 1), 16);
        assert_eq!(scalable.distance(600, 1), 88);
        assert_eq!(threshold.distance(48, 1), 30);
        assert_eq!(scaled.distance(48, 1), 0);
    }

    #[test]
    fn reads_values_from_ini_groups() {
        let kdeglobals = "[General]\nTheme=ignored\n\n[Icons]\nTheme=breeze-dark\n";

        assert_eq!(ini_value(kdeglobals, "Icons", "Theme").as_deref(), Some("breeze-dark"));
        assert_eq!(ini_value(kdeglobals, "Icons", "Missing"), None);
    }

    struct TempTree(PathBuf);

    impl TempTree {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique: u32 = NEXT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("rusty-start-icons-{}-{unique}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("temp dir is writable");
            Self(root)
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().expect("has a parent")).expect("temp dir is writable");
            fs::write(path, contents).expect("temp dir is writable");
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn lookup(tree: &TempTree) -> IconLookup {
        IconLookup::new("test", vec![tree.0.join("icons")], vec![tree.0.join("pixmaps")])
    }

    #[test]
    fn prefers_an_exact_size_in_the_current_theme() {
        let tree = TempTree::new();
        tree.write("icons/test/index.theme", INDEX);
        tree.write("icons/test/16x16/apps/editor.png", "");
        tree.write("icons/test/48x48/apps/editor.png", "");

        let found = lookup(&tree).find("editor", 48).expect("icon found");
        assert_eq!(found, tree.0.join("icons/test/48x48/apps/editor.png"));
    }

    #[test]
    fn falls_back_to_the_closest_size_then_parents_then_hicolor_then_pixmaps() {
        let tree = TempTree::new();
        tree.write("icons/test/index.theme", INDEX);
        tree.write("icons/parent/index.theme", "[Icon Theme]\nDirectories=apps\n\n[apps]\nSize=48\n");
        tree.write("icons/hicolor/index.theme", "[Icon Theme]\nDirectories=apps\n\n[apps]\nSize=48\n");
        tree.write("icons/test/16x16/apps/small.png", "");
        tree.write("icons/parent/apps/inherited.svg", "");
        tree.write("icons/hicolor/apps/vendor.png", "");
        tree.write("pixmaps/legacy.png", "");
        let mut lookup = lookup(&tree);

        assert_eq!(lookup.find("small", 48), Some(tree.0.join("icons/test/16x16/apps/small.png")));
        assert_eq!(lookup.find("inherited", 48), Some(tree.0.join("icons/parent/apps/inherited.svg")));
        assert_eq!(lookup.find("vendor", 48), Some(tree.0.join("icons/hicolor/apps/vendor.png")));
        assert_eq!(lookup.find("legacy.png", 48), Some(tree.0.join("pixmaps/legacy.png")));
        assert_eq!(lookup.find("missing", 48), None);
    }
}
