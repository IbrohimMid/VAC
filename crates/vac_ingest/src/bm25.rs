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
    let re =
        RE.get_or_init(|| Regex::new(r"[A-Za-z][A-Za-z0-9]+").expect("bm25 tokenizer compiles"));
    re.find_iter(text)
        .map(|m| m.as_str().to_ascii_lowercase())
        .collect()
}

fn tokenize_path(p: &Path) -> Vec<String> {
    tokenize(&p.to_string_lossy())
}

/// Rank `paths` against `query`. Returns at most `top_k` entries
/// sorted by descending score. Returns an empty vec if the query
/// has no alphanumeric tokens or the corpus is empty.
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

#[derive(Debug, Clone)]
pub struct Bm25Index {
    pub docs: Vec<(PathBuf, Vec<String>)>,
}

impl Bm25Index {
    pub fn build(paths: &[PathBuf]) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut docs = Vec::new();
        for p in paths {
            if seen.insert(p.as_path()) {
                let tokens = tokenize_path(p);
                docs.push((p.clone(), tokens));
            }
        }
        Self { docs }
    }

    pub fn write_to_file(&self, out_path: &Path) -> std::io::Result<()> {
        let mut w = BufWriter::new(File::create(out_path)?);
        w.write_all(&[1])?; // version
        w.write_all(&(self.docs.len() as u32).to_le_bytes())?;
        for (p, tokens) in &self.docs {
            let p_str = p.to_string_lossy();
            let p_bytes = p_str.as_bytes();
            w.write_all(&(p_bytes.len() as u16).to_le_bytes())?;
            w.write_all(p_bytes)?;

            w.write_all(&(tokens.len() as u16).to_le_bytes())?;
            for t in tokens {
                let t_bytes = t.as_bytes();
                w.write_all(&(t_bytes.len() as u8).to_le_bytes())?;
                w.write_all(t_bytes)?;
            }
        }
        w.flush()
    }

    pub fn read_from_file(in_path: &Path) -> std::io::Result<Self> {
        let mut r = BufReader::new(File::open(in_path)?);
        let mut ver = [0u8; 1];
        r.read_exact(&mut ver)?;
        if ver[0] != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unsupported version",
            ));
        }

        let mut num_buf = [0u8; 4];
        r.read_exact(&mut num_buf)?;
        let num_docs = u32::from_le_bytes(num_buf);

        let mut docs = Vec::with_capacity(num_docs as usize);
        for _ in 0..num_docs {
            let mut len_buf = [0u8; 2];
            r.read_exact(&mut len_buf)?;
            let p_len = u16::from_le_bytes(len_buf) as usize;
            let mut p_bytes = vec![0u8; p_len];
            r.read_exact(&mut p_bytes)?;
            let path = PathBuf::from(String::from_utf8_lossy(&p_bytes).into_owned());

            r.read_exact(&mut len_buf)?;
            let num_tokens = u16::from_le_bytes(len_buf) as usize;
            let mut tokens = Vec::with_capacity(num_tokens);
            for _ in 0..num_tokens {
                let mut t_len_buf = [0u8; 1];
                r.read_exact(&mut t_len_buf)?;
                let t_len = t_len_buf[0] as usize;
                let mut t_bytes = vec![0u8; t_len];
                r.read_exact(&mut t_bytes)?;
                tokens.push(String::from_utf8_lossy(&t_bytes).into_owned());
            }
            docs.push((path, tokens));
        }
        Ok(Self { docs })
    }

    pub fn rank(&self, query: &str, top_k: usize, params: Bm25Params) -> Vec<RankedPath> {
        if self.docs.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let q_tokens = tokenize(query);
        if q_tokens.is_empty() {
            return Vec::new();
        }

        let n = self.docs.len() as f32;
        let avgdl = if self.docs.is_empty() {
            1.0
        } else {
            self.docs.iter().map(|(_, d)| d.len()).sum::<usize>() as f32 / self.docs.len() as f32
        };

        // Document frequencies.
        let mut df: HashMap<&str, usize> = HashMap::new();
        for (_, doc) in &self.docs {
            let unique: std::collections::HashSet<&str> = doc.iter().map(|s| s.as_str()).collect();
            for t in unique {
                *df.entry(t).or_insert(0) += 1;
            }
        }

        let mut scored: Vec<RankedPath> = self
            .docs
            .iter()
            .map(|(p, doc)| {
                let dl = doc.len() as f32;
                let mut score = 0.0_f32;
                for q in &q_tokens {
                    let f = doc.iter().filter(|t| *t == q).count() as f32;
                    if f == 0.0 {
                        continue;
                    }
                    let dfv = df.get(q.as_str()).copied().unwrap_or(0) as f32;
                    let idf = ((n - dfv + 0.5) / (dfv + 0.5) + 1.0).ln();
                    let denom = f + params.k1 * (1.0 - params.b + params.b * (dl / avgdl));
                    let numer = f * (params.k1 + 1.0);
                    score += idf * (numer / denom);
                }
                RankedPath {
                    path: p.clone(),
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
}

/// Staleness check for persisted indexes. Returns `true` when the
/// cache file is at least as new as every source path (and all paths
/// still exist). The caller is expected to rebuild + `write_to_file`
/// when this returns `false`. Missing cache → `false`. Missing source
/// → `false` (a deleted file is a corpus change).
pub fn is_cache_fresh(cache_path: &Path, sources: &[PathBuf]) -> bool {
    let Ok(cache_meta) = std::fs::metadata(cache_path) else {
        return false;
    };
    let Ok(cache_mtime) = cache_meta.modified() else {
        return false;
    };
    for src in sources {
        let Ok(src_meta) = std::fs::metadata(src) else {
            return false;
        };
        let Ok(src_mtime) = src_meta.modified() else {
            return false;
        };
        if src_mtime > cache_mtime {
            return false;
        }
    }
    true
}

pub fn rank_paths(
    paths: &[PathBuf],
    query: &str,
    top_k: usize,
    params: Bm25Params,
) -> Vec<RankedPath> {
    let index = Bm25Index::build(paths);
    index.rank(query, top_k, params)
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
    fn cache_fresh_when_sources_older() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.rs");
        std::fs::write(&src, "x").unwrap();
        // Ensure source mtime is captured before cache.
        std::thread::sleep(std::time::Duration::from_millis(10));
        let cache = tmp.path().join("bm25.bin");
        std::fs::write(&cache, "y").unwrap();
        assert!(is_cache_fresh(&cache, &[src]));
    }

    #[test]
    fn cache_stale_when_source_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("bm25.bin");
        std::fs::write(&cache, "y").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let src = tmp.path().join("a.rs");
        std::fs::write(&src, "x").unwrap();
        assert!(!is_cache_fresh(&cache, &[src]));
    }

    #[test]
    fn cache_stale_when_cache_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.rs");
        std::fs::write(&src, "x").unwrap();
        assert!(!is_cache_fresh(&tmp.path().join("nope.bin"), &[src]));
    }

    #[test]
    fn cache_stale_when_source_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("bm25.bin");
        std::fs::write(&cache, "y").unwrap();
        assert!(!is_cache_fresh(&cache, &[tmp.path().join("gone.rs")]));
    }

    #[test]
    fn repeated_term_in_path_boosts_score() {
        let corpus = vec![p("auth/auth_helpers/auth.rs"), p("auth/utils.rs")];
        let out = rank_paths(&corpus, "auth", 5, Bm25Params::default());
        assert_eq!(out[0].path, p("auth/auth_helpers/auth.rs"));
        assert!(out[0].score > out[1].score);
    }
}
