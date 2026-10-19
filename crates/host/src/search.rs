//! Ranks installed apps against a search query. Platform independent and allocation-light so it
//! can run on the UI thread for every keystroke.

use crate::apps::App;

/// How well an app name matches a query. Earlier variants rank higher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Exact,
    Prefix,
    WordPrefix,
    Initials,
    Substring,
}

/// Lowercased forms of an app name, computed once when the index is built.
struct SearchKey {
    name: String,
    words: Vec<String>,
    initials: String,
}

impl SearchKey {
    fn new(name: &str) -> Self {
        let words: Vec<String> = split_words(name).into_iter().map(str::to_lowercase).collect();
        let initials: String = words.iter().filter_map(|word| word.chars().next()).collect();

        Self { name: name.to_lowercase(), words, initials }
    }

    fn rank(&self, query: &str, query_words: &[&str]) -> Option<Rank> {
        if self.name == query {
            Some(Rank::Exact)
        } else if self.name.starts_with(query) {
            Some(Rank::Prefix)
        } else if query_words.iter().all(|query_word| self.words.iter().any(|word| word.starts_with(query_word))) {
            Some(Rank::WordPrefix)
        } else if query_words.len() == 1 && self.initials.starts_with(query) {
            Some(Rank::Initials)
        } else if self.name.contains(query) {
            Some(Rank::Substring)
        } else {
            None
        }
    }
}

/// Splits a display name into words at separators and at lower-to-upper case changes, so
/// "PowerShell 7 (x64)" yields "Power", "Shell", "7" and "x64".
fn split_words(name: &str) -> Vec<&str> {
    let mut words: Vec<&str> = Vec::new();
    let mut start: Option<usize> = None;
    let mut previous_lowercase = false;

    for (offset, character) in name.char_indices() {
        if !character.is_alphanumeric() {
            if let Some(word_start) = start.take() {
                words.push(&name[word_start..offset]);
            }
            previous_lowercase = false;
            continue;
        }

        if character.is_uppercase() && previous_lowercase {
            if let Some(word_start) = start {
                words.push(&name[word_start..offset]);
            }
            start = Some(offset);
        } else if start.is_none() {
            start = Some(offset);
        }

        previous_lowercase = character.is_lowercase();
    }

    if let Some(word_start) = start {
        words.push(&name[word_start..]);
    }

    words
}

/// Installed apps sorted by name, with precomputed search keys.
#[derive(Default)]
pub struct Index {
    apps: Vec<App>,
    keys: Vec<SearchKey>,
}

impl Index {
    pub fn new(mut apps: Vec<App>) -> Self {
        apps.sort_by_cached_key(|app| app.name.to_lowercase());
        let keys: Vec<SearchKey> = apps.iter().map(|app| SearchKey::new(&app.name)).collect();

        Self { apps, keys }
    }

    pub fn app(&self, position: usize) -> Option<&App> {
        self.apps.get(position)
    }

    /// Returns the positions of matching apps, best match first. An empty query matches every
    /// app in alphabetical order.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query: String = query.trim().to_lowercase();
        if query.is_empty() {
            return (0..self.apps.len()).collect();
        }

        let query_words: Vec<&str> = query.split_whitespace().collect();
        let mut matches: Vec<(Rank, usize)> = self
            .keys
            .iter()
            .enumerate()
            .filter_map(|(position, key)| key.rank(&query, &query_words).map(|rank| (rank, position)))
            .collect();

        // Positions are already alphabetical, so a stable sort keeps that as the final tiebreak.
        matches.sort_by_key(|&(rank, position)| (rank, self.keys[position].name.len()));

        matches.into_iter().map(|(_, position)| position).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(names: &[&str]) -> Index {
        Index::new(names.iter().map(|name| App { name: name.to_string(), launch_id: name.to_string() }).collect())
    }

    fn search(index: &Index, query: &str) -> Vec<String> {
        index.search(query).into_iter().map(|position| index.app(position).unwrap().name.clone()).collect()
    }

    #[test]
    fn empty_query_lists_every_app_alphabetically() {
        let index = index(&["paint", "Calculator", "Notepad"]);

        assert_eq!(search(&index, "  "), ["Calculator", "Notepad", "paint"]);
    }

    #[test]
    fn exact_match_ranks_above_prefix_match() {
        let index = index(&["Notepad++", "Notepad"]);

        assert_eq!(search(&index, "notepad"), ["Notepad", "Notepad++"]);
    }

    #[test]
    fn prefix_ranks_above_word_prefix_and_substring() {
        let index = index(&["Visual Studio Code", "Codec Pack", "Unicode Viewer"]);

        assert_eq!(search(&index, "code"), ["Codec Pack", "Visual Studio Code", "Unicode Viewer"]);
    }

    #[test]
    fn shorter_names_win_within_a_rank() {
        let index = index(&["Microsoft Edge Dev", "Microsoft Edge"]);

        assert_eq!(search(&index, "edge"), ["Microsoft Edge", "Microsoft Edge Dev"]);
    }

    #[test]
    fn every_query_word_must_start_a_name_word() {
        let index = index(&["Visual Studio Code", "Visual Studio 2026", "Android Studio"]);

        assert_eq!(search(&index, "stu cod"), ["Visual Studio Code"]);
    }

    #[test]
    fn matches_initials_including_case_changes() {
        let index = index(&["Visual Studio Code", "PowerShell", "Paint"]);

        assert_eq!(search(&index, "vsc"), ["Visual Studio Code"]);
        assert_eq!(search(&index, "ps"), ["PowerShell"]);
    }

    #[test]
    fn matching_ignores_case_and_surrounding_whitespace() {
        let index = index(&["Calculator"]);

        assert_eq!(search(&index, "  CALC "), ["Calculator"]);
    }

    #[test]
    fn unmatched_query_returns_nothing() {
        let index = index(&["Calculator", "Notepad"]);

        assert!(search(&index, "zzz").is_empty());
    }

    #[test]
    fn splits_words_at_separators_and_case_changes() {
        let words: Vec<&str> = split_words("PowerShell 7 (x64) - preview");

        assert_eq!(words, ["Power", "Shell", "7", "x64", "preview"]);
    }
}
