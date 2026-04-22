use regex::RegexSet;
use serde::{Deserialize, Serialize};

/// Coarse classification of a line's signal value. Distillers use this to
/// decide whether to keep, collapse, or drop a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreClass {
    /// Critical signal: errors, panics, failures. Always keep.
    High,
    /// Useful signal: warnings, state changes, tool boundaries.
    Medium,
    /// Default / neutral.
    Low,
    /// Noise: progress bars, heartbeats, repetitive frames. Safe to drop.
    Noise,
}

pub trait Scorer: Send + Sync {
    fn score(&self, line: &str) -> ScoreClass;
}

/// Regex-based scorer inspired by OMNI's TOML filter DSL, but compiled into
/// three [`RegexSet`]s (high/medium/noise) for O(1) classification.
#[derive(Debug, Clone)]
pub struct RegexScorer {
    high: Option<RegexSet>,
    medium: Option<RegexSet>,
    noise: Option<RegexSet>,
}

impl RegexScorer {
    pub fn new(high: &[&str], medium: &[&str], noise: &[&str]) -> Result<Self, regex::Error> {
        let build = |p: &[&str]| -> Result<Option<RegexSet>, regex::Error> {
            if p.is_empty() { Ok(None) } else { Ok(Some(RegexSet::new(p)?)) }
        };
        Ok(Self { high: build(high)?, medium: build(medium)?, noise: build(noise)? })
    }

    /// Default heuristics: error/panic/fatal → high; warn/deprecated → medium;
    /// progress bars and escape-heavy redraws → noise.
    pub fn default_heuristics() -> Self {
        Self::new(
            &[r"(?i)\b(error|panic(ked)?|fatal|fail(ed|ure)?)\b", r"(?i)\b(traceback|segfault)\b"],
            &[r"(?i)\b(warn(ing)?|deprecated|retry|timeout)\b"],
            &[r"^\s*$", r"\x1b\[\d*[A-Za-z]", r"^\[#+\s*\]"],
        )
        .expect("default heuristics regexes must compile")
    }
}

impl Scorer for RegexScorer {
    fn score(&self, line: &str) -> ScoreClass {
        if self.high.as_ref().is_some_and(|s| s.is_match(line)) {
            return ScoreClass::High;
        }
        if self.medium.as_ref().is_some_and(|s| s.is_match(line)) {
            return ScoreClass::Medium;
        }
        if self.noise.as_ref().is_some_and(|s| s.is_match(line)) {
            return ScoreClass::Noise;
        }
        ScoreClass::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_classifies_errors_as_high() {
        let s = RegexScorer::default_heuristics();
        assert_eq!(s.score("Error: boom"), ScoreClass::High);
        assert_eq!(s.score("thread panicked at ..."), ScoreClass::High);
    }

    #[test]
    fn default_classifies_warn_as_medium() {
        let s = RegexScorer::default_heuristics();
        assert_eq!(s.score("WARN: something"), ScoreClass::Medium);
    }

    #[test]
    fn default_classifies_blank_as_noise() {
        let s = RegexScorer::default_heuristics();
        assert_eq!(s.score(""), ScoreClass::Noise);
        assert_eq!(s.score("   "), ScoreClass::Noise);
    }

    #[test]
    fn default_classifies_plain_as_low() {
        let s = RegexScorer::default_heuristics();
        assert_eq!(s.score("built foo in 0.3s"), ScoreClass::Low);
    }
}
