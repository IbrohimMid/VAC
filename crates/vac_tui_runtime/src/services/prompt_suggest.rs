//! F7.5 — Prompt suggestions.
//!
//! As the operator types, the input footer shows up to N completions
//! drawn from recent prompts in the current session (and optionally
//! across the project). Completions are ranked by:
//!
//! 1. Exact prefix match on the current input (case-insensitive).
//! 2. Recency — newer prompts outrank older ones.
//! 3. Length penalty — very long recalls bubble down so short,
//!    targeted suggestions surface first.

use std::collections::VecDeque;

/// Bounded history of previously-submitted prompts.
#[derive(Debug, Clone)]
pub struct PromptHistory {
    capacity: usize,
    entries: VecDeque<String>,
}

impl PromptHistory {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Record a submitted prompt. Empty/whitespace-only prompts are
    /// dropped. Consecutive duplicates collapse so the same prompt
    /// submitted twice doesn't push the earlier history out.
    pub fn record(&mut self, prompt: impl Into<String>) {
        let p = prompt.into();
        let trimmed = p.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.entries.front().map(|s| s.as_str()) == Some(trimmed) {
            return;
        }
        if self.entries.len() >= self.capacity {
            self.entries.pop_back();
        }
        self.entries.push_front(trimmed.to_string());
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Top-`limit` suggestions for `input`. Empty input returns the
    /// most recent prompts (MRU list). Case-insensitive prefix match.
    pub fn suggest(&self, input: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let needle = input.trim().to_lowercase();
        let mut scored: Vec<(f32, usize, &String)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                needle.is_empty() || p.to_lowercase().starts_with(&needle)
            })
            .map(|(i, p)| {
                // Lower score wins. Recency weighs more than length.
                let recency = i as f32; // 0 = newest
                let length_penalty = (p.len() as f32).ln().max(0.0) * 0.1;
                (recency + length_penalty, i, p)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        // Dedup non-consecutive duplicates. `record` already
        // collapses consecutive repeats, but history like
        // ["x", "y", "x"] surfaces "x" twice without this pass.
        let mut seen = std::collections::HashSet::new();
        scored
            .into_iter()
            .filter_map(|(_, _, p)| {
                if seen.insert(p.clone()) {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .take(limit)
            .collect()
    }
}

impl Default for PromptHistory {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_drops_empty_and_whitespace() {
        let mut h = PromptHistory::with_capacity(4);
        h.record("");
        h.record("   ");
        h.record("  real  ");
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn record_collapses_consecutive_duplicates() {
        let mut h = PromptHistory::with_capacity(4);
        h.record("do x");
        h.record("do x");
        h.record("do y");
        h.record("do y");
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn suggest_empty_input_returns_mru() {
        let mut h = PromptHistory::with_capacity(4);
        h.record("first");
        h.record("second");
        h.record("third");
        let s = h.suggest("", 10);
        assert_eq!(s, vec!["third", "second", "first"]);
    }

    #[test]
    fn suggest_prefix_match_is_case_insensitive() {
        let mut h = PromptHistory::with_capacity(8);
        h.record("Refactor auth module");
        h.record("Run cargo nextest");
        h.record("Review diff");
        let s = h.suggest("re", 10);
        assert!(s.iter().any(|p| p == "Refactor auth module"));
        assert!(s.iter().any(|p| p == "Review diff"));
        assert!(!s.iter().any(|p| p == "Run cargo nextest"));
    }

    #[test]
    fn suggest_recency_wins_over_length() {
        let mut h = PromptHistory::with_capacity(4);
        h.record("re a very long prompt that should rank lower because old");
        h.record("re new short");
        let s = h.suggest("re", 10);
        assert_eq!(s[0], "re new short");
    }

    #[test]
    fn suggest_respects_limit() {
        let mut h = PromptHistory::with_capacity(10);
        for i in 0..10 {
            h.record(format!("item {i}"));
        }
        assert_eq!(h.suggest("item", 3).len(), 3);
        assert_eq!(h.suggest("item", 0).len(), 0);
    }

    #[test]
    fn suggest_dedups_non_consecutive_duplicates() {
        let mut h = PromptHistory::with_capacity(8);
        h.record("x");
        h.record("y");
        h.record("x"); // non-consecutive repeat
        let s = h.suggest("", 10);
        assert_eq!(s.len(), 2, "got {s:?}");
        // Most recent "x" wins and appears once.
        assert_eq!(s[0], "x");
        assert_eq!(s[1], "y");
    }

    #[test]
    fn capacity_drops_oldest() {
        let mut h = PromptHistory::with_capacity(2);
        h.record("a");
        h.record("b");
        h.record("c");
        assert_eq!(h.suggest("", 10), vec!["c", "b"]);
    }
}
