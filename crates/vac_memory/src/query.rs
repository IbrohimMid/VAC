//! Relevance scoring — tf-idf over the memory corpus plus a recency
//! bump. Deliberately simple: small corpora (100s of files), no vector
//! index, all in-memory. Drivers call [`find_relevant`] on a fresh
//! scan result.

use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::memdir::{Memory, MemoryKind};

/// Score triple returned alongside a memory hit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RelevanceScore {
    /// Raw tf-idf score over (title + topic + tags + body).
    pub tfidf: f32,
    /// Recency multiplier in (0.0, 1.0]: 1.0 = today, ~0.5 = 30 days.
    pub recency: f32,
    /// Blended score: `tfidf * recency * importance`.
    pub blended: f32,
}

fn tokenize(text: &str) -> Vec<String> {
    static TOK_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = TOK_RE.get_or_init(|| {
        Regex::new(r"[A-Za-z][A-Za-z0-9_\-]{1,}").expect("tokenizer regex compiles")
    });
    re.find_iter(text)
        .map(|m| m.as_str().to_ascii_lowercase())
        .collect()
}

fn recency_weight(ts: chrono::DateTime<chrono::Utc>) -> f32 {
    let days = (chrono::Utc::now() - ts).num_days().max(0) as f32;
    // Half-life ≈ 30 days. `2^(-days/30)`.
    (-(days / 30.0) * std::f32::consts::LN_2).exp()
}

/// Score + top-k over the given memory set.
///
/// - `query` is tokenized and scored against each memory's
///   `corpus_text()`.
/// - Archived memories are excluded unless `include_archived` is true.
/// - `k` is the top-k cutoff; `0` means "all".
pub fn find_relevant(
    memories: &[Memory],
    query: &str,
    k: usize,
    include_archived: bool,
) -> Vec<(Memory, RelevanceScore)> {
    let pool: Vec<&Memory> = memories
        .iter()
        .filter(|m| include_archived || m.kind != MemoryKind::Archived)
        .collect();
    if pool.is_empty() {
        return Vec::new();
    }
    let q_tokens = tokenize(query);
    if q_tokens.is_empty() {
        // No query — return by recency alone.
        let mut scored: Vec<(Memory, RelevanceScore)> = pool
            .iter()
            .map(|m| {
                let r = recency_weight(m.effective_timestamp());
                (
                    (*m).clone(),
                    RelevanceScore {
                        tfidf: 0.0,
                        recency: r,
                        blended: r * m.frontmatter.importance,
                    },
                )
            })
            .collect();
            scored.sort_by(|a, b| b.1.blended.partial_cmp(&a.1.blended).unwrap_or(std::cmp::Ordering::Equal));
        if k > 0 {
            scored.truncate(k);
        }
        return scored;
    }

    // tf-idf. Document frequencies over the pool.
    let mut df: HashMap<String, usize> = HashMap::new();
    let mut docs: Vec<(Vec<String>, &Memory)> = Vec::with_capacity(pool.len());
    for m in &pool {
        let toks = tokenize(&m.corpus_text());
        let uniq: std::collections::HashSet<_> = toks.iter().cloned().collect();
        for t in uniq {
            *df.entry(t).or_insert(0) += 1;
        }
        docs.push((toks, *m));
    }
    let n = pool.len() as f32;

    let mut scored: Vec<(Memory, RelevanceScore)> = docs
        .into_iter()
        .map(|(toks, m)| {
            let mut tf_counts: HashMap<&str, u32> = HashMap::new();
            for t in &toks {
                *tf_counts.entry(t).or_insert(0) += 1;
            }
            let total = toks.len().max(1) as f32;
            let mut tfidf = 0.0_f32;
            for q in &q_tokens {
                let tf = tf_counts.get(q.as_str()).copied().unwrap_or(0) as f32 / total;
                if tf == 0.0 {
                    continue;
                }
                let dfv = df.get(q).copied().unwrap_or(1) as f32;
                let idf = ((n + 1.0) / (dfv + 1.0)).ln() + 1.0;
                tfidf += tf * idf;
            }
            let recency = recency_weight(m.effective_timestamp());
            let blended = tfidf * recency * m.frontmatter.importance;
            (
                m.clone(),
                RelevanceScore {
                    tfidf,
                    recency,
                    blended,
                },
            )
        })
        .filter(|(_, s)| s.tfidf > 0.0)
        .collect();

    scored.sort_by(|a, b| b.1.blended.partial_cmp(&a.1.blended).unwrap_or(std::cmp::Ordering::Equal));
    if k > 0 {
        scored.truncate(k);
    }
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memdir::MemoryFrontmatter;
    use std::path::PathBuf;

    fn m(topic: &str, body: &str, days_ago: i64, importance: f32) -> Memory {
        Memory {
            kind: MemoryKind::Active,
            path: PathBuf::from(format!("{topic}.md")),
            frontmatter: MemoryFrontmatter {
                topic: topic.into(),
                title: None,
                tags: vec![],
                created_at: chrono::Utc::now() - chrono::Duration::days(days_ago),
                updated_at: None,
                importance,
                source_policy: None,
            },
            body: body.into(),
        }
    }

    #[test]
    fn tokenizer_lowercases_and_splits() {
        let toks = tokenize("Hello, world — rust_lang.");
        assert!(toks.contains(&"hello".to_string()));
        assert!(toks.contains(&"world".to_string()));
        assert!(toks.contains(&"rust_lang".to_string()));
    }

    #[test]
    fn find_relevant_matches_body_terms() {
        let pool = vec![
            m("a", "cargo test is blocked; always use nextest", 1, 1.0),
            m("b", "TUI renders workbench tabs", 10, 1.0),
            m("c", "cargo build takes 20 minutes", 20, 1.0),
        ];
        let hits = find_relevant(&pool, "nextest", 10, false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.frontmatter.topic, "a");
    }

    #[test]
    fn recency_breaks_ties() {
        let pool = vec![
            m("old", "cargo cargo cargo", 60, 1.0),
            m("new", "cargo cargo cargo", 1, 1.0),
        ];
        let hits = find_relevant(&pool, "cargo", 10, false);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0.frontmatter.topic, "new");
    }

    #[test]
    fn importance_boost_matters() {
        let pool = vec![
            m("dim", "cargo test", 1, 0.1),
            m("bright", "cargo test", 1, 1.0),
        ];
        let hits = find_relevant(&pool, "cargo", 10, false);
        assert_eq!(hits[0].0.frontmatter.topic, "bright");
    }

    #[test]
    fn archived_excluded_by_default() {
        let mut arch = m("arch", "cargo test", 1, 1.0);
        arch.kind = MemoryKind::Archived;
        let pool = vec![arch];
        assert!(find_relevant(&pool, "cargo", 10, false).is_empty());
        assert_eq!(find_relevant(&pool, "cargo", 10, true).len(), 1);
    }

    #[test]
    fn empty_query_ranks_by_recency() {
        let pool = vec![m("a", "x", 30, 1.0), m("b", "y", 1, 1.0)];
        let hits = find_relevant(&pool, "", 10, false);
        assert_eq!(hits[0].0.frontmatter.topic, "b");
    }
}
