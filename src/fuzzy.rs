//! Fuzzy matching for completion, the command palette and quick open.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Score an identifier against a typed prefix: exact prefixes rank highest, then
/// subsequence matches (favoring matches at `-`/`/` word boundaries). None if no match.
pub fn score(query: &str, candidate: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let c: Vec<char> = candidate.chars().flat_map(|c| c.to_lowercase()).collect();
    if c.starts_with(&q) {
        return Some(1000 - c.len() as i64);
    }
    let mut score = 0i64;
    let mut qi = 0;
    let mut prev_match = false;
    for (i, ch) in c.iter().enumerate() {
        if qi < q.len() && *ch == q[qi] {
            let boundary = i == 0 || matches!(c[i - 1], '-' | '/' | ':' | '*' | '>' | '!' | '?');
            score += if boundary { 30 } else if prev_match { 15 } else { 5 };
            qi += 1;
            prev_match = true;
        } else {
            prev_match = false;
        }
    }
    (qi == q.len()).then_some(score - c.len() as i64)
}

/// A reusable fuzzy matcher for longer lists (files, commands), backed by nucleo.
pub struct FuzzyMatcher {
    matcher: Matcher,
    buf: Vec<char>,
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self { matcher: Matcher::new(Config::DEFAULT.match_paths()), buf: Vec::new() }
    }
}

impl FuzzyMatcher {
    /// Rank `items` by `query`, returning (index, score, matched char indices).
    pub fn rank<'a>(&mut self, query: &str, items: impl Iterator<Item = &'a str>) -> Vec<(usize, u32, Vec<u32>)> {
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut out = Vec::new();
        for (i, item) in items.enumerate() {
            let hay = Utf32Str::new(item, &mut self.buf);
            let mut idx = Vec::new();
            if let Some(s) = pattern.indices(hay, &mut self.matcher, &mut idx) {
                idx.sort_unstable();
                idx.dedup();
                out.push((i, s, idx));
            }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_beats_subsequence() {
        assert!(score("str", "string-append").unwrap() > score("str", "list-ref-tr").unwrap_or(-1));
        assert!(score("sa", "string-append").is_some());
        assert!(score("xyz", "map").is_none());
    }

    #[test]
    fn matcher_ranks_paths() {
        let mut m = FuzzyMatcher::default();
        let items = ["src/main.rkt", "tests/main-test.rkt", "README.md"];
        let r = m.rank("main", items.iter().copied());
        assert_eq!(r.len(), 2);
    }
}
