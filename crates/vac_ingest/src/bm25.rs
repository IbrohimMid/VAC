//! F9.5 — BM25 ranker for file paths.
//!
//! `build_file_index` returns paths in filesystem order, which is not
//! useful when a caller wants "most-relevant-to-this-query-token"
//! ordering (e.g. `@file` picker narrowing by an operator-supplied
//! hint). This module scores the existing path list against a query
//! using the standard BM25 formulation with per-path path-segment
//! tokenization.
//!
//! Pure in-memory, no persistence — the index is rebuilt on every
//! call. At O(N*Q) for a corpus of a few thousand paths this is fast
//! enough for interactive use; a persistent inverted index is out of
//! scope for this milestone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;

/// Classic BM25 params. `k1` controls term-frequency saturation;
/// `b` controls length normalization.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Bm25Params {
    pub k1: f32,
    pub b: f32,
}

impl Default for Bm25Params {
    fn default() -> Self {
        // Textbook defaults (Okapi BM25).
        Self { k1: 1.5, b: 0.75 }
    }
}

/// Hit returned from [`rank_paths`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RankedPath {
    pub path: PathBuf,
    pub score: f32,
}

fn tokenize(text: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"[A-Za-z][A-Za-z0-9]+").expect("bm25 tokenizer compiles")
    });
    re.find_iter(text).map(|m| m.as_str().to_ascii_lowercase()).collect()
}

fn tokenize_path(p: &Path) -> Vec<String> {
    tokenize(&p.to_string_lossy())
}

/// Rank `paths` against `query`. Returns at most `top_k` entries
/// sorted by descending score. Returns an empty vec if the query
/// has no alphanumeric tokens or the corpus is empty.
pub fn rank_paths(
    paths: &[PathBuf],
    query: &str,
    top_k: usize,
    params: Bm25Params,
) -> Vec<RankedPath> {
    if paths.is_empty() || top_k == 0 {
        return Vec::new();
    }
    let q_tokens = tokenize(query);
    if q_tokens.is_empty() {
        return Vec::new();
    }

    // Dedup paths before scoring. Duplicates would otherwise inflate
    // document-frequency (depressing IDF for their shared terms) AND
    // produce duplicate `RankedPath` rows. Order-preserving so the
    // first occurrence wins.
    let mut seen: std::collections::HashSet<&Path> = std::collections::HashSet::new();
    let paths: Vec<PathBuf> = paths
        .iter()
        .filter(|p| seen.insert(p.as_path()))
        .cloned()
        .collect();
    let paths = paths.as_slice();

    // Tokenize corpus once.
    let docs: Vec<Vec<String>> = paths.iter().map(|p| tokenize_path(p)).collect();
    let n = docs.len() as f32;
    let avgdl = if docs.is_empty() {
        1.0
    } else {
        docs.iter().map(|d| d.len()).sum::<usize>() as f32 / docs.len() as f32
    };

    // Document frequencies.
    let mut df: HashMap<&str, usize> = HashMap::new();
    for doc in &docs {
        let unique: std::collections::HashSet<&str> =
            doc.iter().map(|s| s.as_str()).collect();
        for t in unique {
            *df.entry(t).or_insert(0) += 1;
        }
    }

    let mut scored: Vec<RankedPath> = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| {
            let dl = doc.len() as f32;
            let mut score = 0.0_f32;
            for q in &q_tokens {
                let f = doc.iter().filter(|t| *t == q).count() as f32;
                if f == 0.0 {
                    continue;
                }
                let dfv = df.get(q.as_str()).copied().unwrap_or(0) as f32;
                // Robertson-Sparck-Jones smoothed IDF.
                let idf = ((n - dfv + 0.5) / (dfv + 0.5) + 1.0).ln();
                let denom = f + params.k1 * (1.0 - params.b + params.b * (dl / avgdl));
                let numer = f * (params.k1 + 1.0);
                score += idf * (numer / denom);
            }
            RankedPath {
                path: paths[i].clone(),
                score,
            }
        })
        .filter(|r| r.score > 0.0)
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(top_k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn empty_corpus_is_empty_result() {
        let out = rank_paths(&[], "anything", 10, Bm25Params::default());
        assert!(out.is_empty());
    }

    #[test]
    fn query_without_alnum_returns_empty() {
        let corpus = vec![p("src/main.rs")];
        assert!(rank_paths(&corpus, "!!!", 10, Bm25Params::default()).is_empty());
    }

    #[test]
    fn matching_term_outranks_non_match() {
        let corpus = vec![
            p("src/auth/mod.rs"),
            p("src/database/users.rs"),
            p("src/ui/header.rs"),
        ];
        let out = rank_paths(&corpus, "auth", 10, Bm25Params::default());
        assert_eq!(out.len(), 1);
        assert!(out[0].path.ends_with("auth/mod.rs"));
    }

    #[test]
    fn rarer_term_scores_higher_via_idf() {
        let corpus = vec![
            p("src/auth/mod.rs"),
            p("src/auth/session.rs"),
            p("src/auth/tokens.rs"),
            p("src/database/users.rs"),
            p("src/database/schema.rs"),
        ];
        // "users" is rare (1 doc), "auth" is common (3 docs).
        let out = rank_paths(&corpus, "users auth", 10, Bm25Params::default());
        // Hit on "users.rs" matches both terms; should rank first.
        assert_eq!(out[0].path, p("src/database/users.rs"));
    }

    #[test]
    fn top_k_caps_results() {
        let corpus = vec![
            p("a/auth.rs"),
            p("b/auth.rs"),
            p("c/auth.rs"),
            p("d/auth.rs"),
        ];
        let out = rank_paths(&corpus, "auth", 2, Bm25Params::default());
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn case_insensitive_tokenization() {
        let corpus = vec![p("src/AUTH/Mod.rs")];
        let out = rank_paths(&corpus, "auth", 5, Bm25Params::default());
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn duplicate_paths_are_deduped_before_scoring() {
        let corpus = vec![
            p("src/auth/mod.rs"),
            p("src/auth/mod.rs"), // exact dupe
            p("src/database/users.rs"),
        ];
        let out = rank_paths(&corpus, "auth", 10, Bm25Params::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, p("src/auth/mod.rs"));
    }

    #[test]
    fn repeated_term_in_path_boosts_score() {
        let corpus = vec![
            p("auth/auth_helpers/auth.rs"),
            p("auth/utils.rs"),
        ];
        let out = rank_paths(&corpus, "auth", 5, Bm25Params::default());
        assert_eq!(out[0].path, p("auth/auth_helpers/auth.rs"));
        assert!(out[0].score > out[1].score);
    }
}
