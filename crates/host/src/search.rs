//! Ranks installed apps and power actions against a search query. Platform independent and
//! allocation-light so it can run on the UI thread for every keystroke.

use std::cmp::Reverse;

/// Anything listed by name in the menu.
pub trait Searchable {
    fn name(&self) -> &str;

    /// Other words the item is found by, such as "terminal" for Konsole. Matches on them rank below
    /// every match on the name except a substring.
    fn keywords(&self) -> &[String] {
        &[]
    }
}

/// How well an item matches a query. Earlier variants rank higher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Exact,
    Prefix,
    WordPrefix,
    Initials,
    /// Every query word starts a keyword word or a name word.
    Keyword,
    Substring,
}

/// Lowercased forms of an app name, computed once when the index is built.
struct SearchKey {
    name: String,
    /// The name without whitespace, so "shutdown" finds "Shut down".
    compact: String,
    words: Vec<String>,
    initials: String,
    /// Words of every keyword, matched by prefix only.
    keyword_words: Vec<String>,
}

impl SearchKey {
    fn new(name: &str, keywords: &[String]) -> Self {
        let words: Vec<String> = split_words(name).into_iter().map(str::to_lowercase).collect();
        let initials: String = words.iter().filter_map(|word| word.chars().next()).collect();
        let mut keyword_words: Vec<String> =
            keywords.iter().flat_map(|keyword| split_words(keyword)).map(str::to_lowercase).collect();
        keyword_words.sort_unstable();
        keyword_words.dedup();

        let name: String = name.to_lowercase();
        let compact: String = name.split_whitespace().collect();

        Self { name, compact, words, initials, keyword_words }
    }

    fn rank(&self, query: &str, query_words: &[&str]) -> Option<Rank> {
        if self.name == query || self.compact == query {
            Some(Rank::Exact)
        } else if self.name.starts_with(query) || self.compact.starts_with(query) {
            Some(Rank::Prefix)
        } else if query_words.iter().all(|query_word| self.words.iter().any(|word| word.starts_with(query_word))) {
            Some(Rank::WordPrefix)
        } else if query_words.len() == 1 && self.initials.starts_with(query) {
            Some(Rank::Initials)
        } else if self.matches_keywords(query_words) {
            Some(Rank::Keyword)
        } else if self.name.contains(query) {
            Some(Rank::Substring)
        } else {
            None
        }
    }

    fn matches_keywords(&self, query_words: &[&str]) -> bool {
        !self.keyword_words.is_empty()
            && query_words
                .iter()
                .all(|query_word| self.keyword_words.iter().chain(&self.words).any(|word| word.starts_with(query_word)))
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

/// Items sorted by name, with precomputed search keys.
pub struct Index<T> {
    items: Vec<T>,
    keys: Vec<SearchKey>,
}

impl<T> Default for Index<T> {
    fn default() -> Self {
        Self { items: Vec::new(), keys: Vec::new() }
    }
}

impl<T: Searchable> Index<T> {
    pub fn new(mut items: Vec<T>) -> Self {
        items.sort_by_cached_key(|item| item.name().to_lowercase());
        let keys: Vec<SearchKey> = items.iter().map(|item| SearchKey::new(item.name(), item.keywords())).collect();

        Self { items, keys }
    }

    pub fn get(&self, position: usize) -> Option<&T> {
        self.items.get(position)
    }

    /// Every position, in alphabetical order.
    pub fn all(&self) -> Vec<usize> {
        (0..self.items.len()).collect()
    }

    /// Returns the positions of matching items, best match first. Within a match quality, a higher
    /// `score` wins, then the shorter name. An empty query matches nothing.
    pub fn search(&self, query: &str, score: impl Fn(&T) -> u64) -> Vec<usize> {
        let query: String = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        let query_words: Vec<&str> = query.split_whitespace().collect();
        let mut matches: Vec<(Rank, Reverse<u64>, usize, usize)> = self
            .keys
            .iter()
            .enumerate()
            .filter_map(|(position, key)| {
                let rank: Rank = key.rank(&query, &query_words)?;
                Some((rank, Reverse(score(&self.items[position])), key.name.len(), position))
            })
            .collect();

        // Positions are already alphabetical, so a stable sort keeps that as the final tiebreak.
        matches.sort_by_key(|&(rank, score, length, _)| (rank, score, length));

        matches.into_iter().map(|(_, _, _, position)| position).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Searchable for String {
        fn name(&self) -> &str {
            self
        }
    }

    fn index(names: &[&str]) -> Index<String> {
        Index::new(names.iter().map(ToString::to_string).collect())
    }

    fn names(index: &Index<String>, positions: Vec<usize>) -> Vec<String> {
        positions.into_iter().map(|position| index.get(position).unwrap().clone()).collect()
    }

    fn search(index: &Index<String>, query: &str) -> Vec<String> {
        names(index, index.search(query, |_| 0))
    }

    struct Tagged {
        name: &'static str,
        keywords: Vec<String>,
    }

    impl Searchable for Tagged {
        fn name(&self) -> &str {
            self.name
        }

        fn keywords(&self) -> &[String] {
            &self.keywords
        }
    }

    fn tagged(items: &[(&'static str, &[&str])]) -> Index<Tagged> {
        Index::new(
            items
                .iter()
                .map(|&(name, keywords)| Tagged { name, keywords: keywords.iter().map(ToString::to_string).collect() })
                .collect(),
        )
    }

    fn search_tagged(index: &Index<Tagged>, query: &str, score: impl Fn(&Tagged) -> u64) -> Vec<&'static str> {
        index.search(query, score).into_iter().map(|position| index.get(position).unwrap().name).collect()
    }

    #[test]
    fn all_lists_every_item_alphabetically() {
        let index = index(&["paint", "Calculator", "Notepad"]);

        assert_eq!(names(&index, index.all()), ["Calculator", "Notepad", "paint"]);
    }

    #[test]
    fn blank_query_matches_nothing() {
        let index = index(&["Calculator"]);

        assert_eq!(search(&index, "  "), Vec::<String>::new());
    }

    #[test]
    fn higher_score_wins_within_a_rank() {
        let index = index(&["Microsoft Edge", "Microsoft Edge Dev"]);
        let positions: Vec<usize> =
            index.search("edge", |name| if name.as_str() == "Microsoft Edge Dev" { 300 } else { 0 });

        assert_eq!(names(&index, positions), ["Microsoft Edge Dev", "Microsoft Edge"]);
    }

    #[test]
    fn score_never_lifts_a_weaker_match() {
        let index = index(&["Codec Pack", "Visual Studio Code"]);
        let positions: Vec<usize> =
            index.search("code", |name| if name.as_str() == "Visual Studio Code" { 300 } else { 0 });

        assert_eq!(names(&index, positions), ["Codec Pack", "Visual Studio Code"]);
    }

    #[test]
    fn matches_names_typed_without_spaces() {
        let index = index(&["Shut down", "Sign out"]);

        assert_eq!(search(&index, "shutdown"), ["Shut down"]);
        assert_eq!(search(&index, "signo"), ["Sign out"]);
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

        assert_eq!(search(&index, "zzz"), Vec::<String>::new());
    }

    #[test]
    fn keyword_match_ranks_below_initials_and_above_substring() {
        let index = tagged(&[("Kate", &[]), ("Konsole", &["Terminal"]), ("Task Explorer", &[])]);

        assert_eq!(search_tagged(&index, "te", |_| 0), ["Task Explorer", "Konsole", "Kate"]);
    }

    #[test]
    fn score_never_lifts_a_keyword_match_over_a_name_match() {
        let index = tagged(&[("Konsole", &["terminal"]), ("Windows Terminal", &[])]);
        let frecency = |item: &Tagged| if item.name == "Konsole" { 300 } else { 0 };

        assert_eq!(search_tagged(&index, "term", frecency), ["Windows Terminal", "Konsole"]);
    }

    #[test]
    fn keywords_never_match_by_substring() {
        let index = tagged(&[("Konsole", &["terminal"])]);

        assert_eq!(search_tagged(&index, "minal", |_| 0), Vec::<&str>::new());
    }

    #[test]
    fn keyword_queries_may_mix_name_and_keyword_words() {
        let index = tagged(&[("Firefox", &["Web Browser"]), ("Chromium", &["Web Browser"])]);

        assert_eq!(search_tagged(&index, "browser", |_| 0), ["Firefox", "Chromium"]);
        assert_eq!(search_tagged(&index, "fire brow", |_| 0), ["Firefox"]);
    }

    #[test]
    fn splits_words_at_separators_and_case_changes() {
        let words: Vec<&str> = split_words("PowerShell 7 (x64) - preview");

        assert_eq!(words, ["Power", "Shell", "7", "x64", "preview"]);
    }
}
