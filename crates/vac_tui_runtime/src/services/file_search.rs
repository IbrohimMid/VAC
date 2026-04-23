use ignore::WalkBuilder;
use nucleo_matcher::{
    Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

use vac_ingest::{Bm25Params, rank_paths};

pub fn build_file_index(project_root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let walker = WalkBuilder::new(project_root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        let Ok(entry) = entry else { continue };
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(project_root).unwrap_or(path);
        let s = rel.to_string_lossy().to_string();
        if s.starts_with(".git/")
            || s.starts_with("target/")
            || s.starts_with(".vac/")
            || s.starts_with(".trae/")
        {
            continue;
        }
        out.push(s);
    }

    out.sort();
    out.dedup();
    out
}

pub fn fuzzy_search_files(query: &str, files: &[String], max_matches: usize) -> Vec<String> {
    let q = query.trim();
    if q.is_empty() {
        return files.iter().take(max_matches).cloned().collect();
    }

    let pattern = Pattern::new(
        q,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut best = BestMatchesList::new(
        max_matches,
        pattern,
        Matcher::new(nucleo_matcher::Config::DEFAULT),
    );
    for f in files {
        best.insert(f);
    }
    best.get_sorted_matches()
}

/// R4 — BM25-ranked file search.
///
/// Fuzzy (nucleo) matching answers "which paths contain these
/// characters in order"; BM25 answers "which paths are most
/// relevant to this token set". For typed queries of ≥3 alphanum
/// characters BM25 surfaces path-segment hits (e.g. `auth` → files
/// under `src/auth/`) that fuzzy scoring ranks lower.
///
/// Strategy here is additive: return the BM25 top-k first, then
/// backfill from the fuzzy result so operators don't lose short-
/// query matches on 1–2 character inputs.
pub fn ranked_search_files(
    query: &str,
    files: &[String],
    max_matches: usize,
    bm25_index: Option<&vac_ingest::Bm25Index>,
) -> Vec<String> {
    let q = query.trim();
    if q.len() < 3 {
        return fuzzy_search_files(query, files, max_matches);
    }
    
    let bm = if let Some(index) = bm25_index {
        index.rank(q, max_matches, Bm25Params::default())
    } else {
        let bm_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        rank_paths(&bm_paths, q, max_matches, Bm25Params::default())
    };

    let mut out: Vec<String> = bm
        .into_iter()
        .map(|r| r.path.to_string_lossy().to_string())
        .collect();
    if out.len() < max_matches {
        for f in fuzzy_search_files(query, files, max_matches) {
            if !out.contains(&f) {
                out.push(f);
                if out.len() >= max_matches {
                    break;
                }
            }
        }
    }
    out.truncate(max_matches);
    out
}

#[derive(Debug)]
struct BestMatchesList {
    max_count: usize,
    pattern: Pattern,
    matcher: Matcher,
    binary_heap: BinaryHeap<Reverse<(u32, String)>>,
    utf32buf: Vec<char>,
}

impl BestMatchesList {
    fn new(max_count: usize, pattern: Pattern, matcher: Matcher) -> Self {
        Self {
            max_count,
            pattern,
            matcher,
            binary_heap: BinaryHeap::new(),
            utf32buf: Vec::new(),
        }
    }

    fn insert(&mut self, file_path: &str) {
        let haystack: Utf32Str<'_> = Utf32Str::new(file_path, &mut self.utf32buf);
        if let Some(score) = self.pattern.score(haystack, &mut self.matcher) {
            if self.binary_heap.len() < self.max_count {
                self.binary_heap
                    .push(Reverse((score, file_path.to_string())));
            } else if let Some(min_element) = self.binary_heap.peek()
                && score > min_element.0.0
            {
                self.binary_heap.pop();
                self.binary_heap
                    .push(Reverse((score, file_path.to_string())));
            }
        }
    }

    fn get_sorted_matches(&mut self) -> Vec<String> {
        let mut sorted_matches: Vec<(u32, String)> = self
            .binary_heap
            .drain()
            .map(|Reverse((score, path))| (score, path))
            .collect();
        sorted_matches.sort_by(|a, b| match b.0.cmp(&a.0) {
            std::cmp::Ordering::Equal => a.1.cmp(&b.1),
            other => other,
        });
        sorted_matches.into_iter().map(|(_, path)| path).collect()
    }
}

#[cfg(test)]
mod ranked_tests {
    use super::*;

    fn corpus() -> Vec<String> {
        vec![
            "src/auth/mod.rs".into(),
            "src/auth/session.rs".into(),
            "src/auth/tokens.rs".into(),
            "src/database/users.rs".into(),
            "src/database/schema.rs".into(),
            "src/ui/header.rs".into(),
        ]
    }

    #[test]
    fn short_query_falls_back_to_fuzzy() {
        // <3 chars: path through nucleo, not BM25.
        let out = ranked_search_files("au", &corpus(), 5, None);
        assert!(!out.is_empty());
    }

    #[test]
    fn bm25_wins_on_token_relevance() {
        let out = ranked_search_files("auth", &corpus(), 10, None);
        // Every auth path shows up; header/database deprioritized.
        assert!(out.iter().any(|p| p.contains("auth/mod.rs")));
        assert!(out.iter().any(|p| p.contains("auth/session.rs")));
        assert!(out.iter().any(|p| p.contains("auth/tokens.rs")));
    }

    #[test]
    fn ranked_backfills_from_fuzzy_when_bm25_underfills() {
        // Query that BM25 can match on ≤ 1 path, but fuzzy matches
        // more. Backfill must honor `max_matches`.
        let out = ranked_search_files("users", &corpus(), 3, None);
        assert!(out.iter().any(|p| p.contains("users.rs")));
        assert!(out.len() <= 3);
    }
}
