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
            if p.is_empty() {
                Ok(None)
            } else {
                Ok(Some(RegexSet::new(p)?))
            }
        };
        Ok(Self {
            high: build(high)?,
            medium: build(medium)?,
            noise: build(noise)?,
        })
    }

    /// Default heuristics: error/panic/fatal → high; warn/deprecated → medium;
    /// progress bars and escape-heavy redraws → noise.
    pub fn default_heuristics() -> Self {
        Self::new(
            &[
                r"(?i)\b(error|panic(ked)?|fatal|fail(ed|ure)?)\b",
                r"(?i)\b(traceback|segfault)\b",
            ],
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

/// Chained scorer: delegate to each inner scorer in turn; the first
/// classification higher than `Low` wins (custom rules override defaults
/// at medium/high/noise, otherwise the tail scorer's judgement stands).
pub struct ChainedScorer {
    scorers: Vec<Box<dyn Scorer>>,
}

impl ChainedScorer {
    pub fn new(scorers: Vec<Box<dyn Scorer>>) -> Self {
        Self { scorers }
    }

    /// Build a `ChainedScorer` from default heuristics followed by custom
    /// rules loaded from `SignalConfig`. Returns `None` if all supplied
    /// patterns fail to compile.
    pub fn from_config(cfg: &crate::config::SignalConfig) -> Result<Self, regex::Error> {
        let default = Box::new(RegexScorer::default_heuristics());
        let mut scorers: Vec<Box<dyn Scorer>> = vec![default];
        if !cfg.custom_filters.is_empty() {
            let custom = CustomRulesScorer::from_rules(&cfg.custom_filters)?;
            scorers.push(Box::new(custom));
        }
        Ok(Self::new(scorers))
    }
}

impl Scorer for ChainedScorer {
    fn score(&self, line: &str) -> ScoreClass {
        let mut result = ScoreClass::Low;
        for s in &self.scorers {
            match s.score(line) {
                // High is authoritative across the chain; later scorers
                // can't downgrade it, so short-circuit.
                ScoreClass::High => return ScoreClass::High,
                c @ (ScoreClass::Medium | ScoreClass::Noise) => result = c,
                ScoreClass::Low => {}
            }
        }
        result
    }
}

/// Scorer built from a list of user-supplied [`FilterRule`]s.
pub struct CustomRulesScorer {
    rules: Vec<(regex::Regex, ScoreClass)>,
}

impl CustomRulesScorer {
    pub fn from_rules(rules: &[crate::config::FilterRule]) -> Result<Self, regex::Error> {
        let mut compiled = Vec::with_capacity(rules.len());
        for r in rules {
            compiled.push((regex::Regex::new(&r.pattern)?, r.class));
        }
        Ok(Self { rules: compiled })
    }
}

impl Scorer for CustomRulesScorer {
    fn score(&self, line: &str) -> ScoreClass {
        for (re, class) in &self.rules {
            if re.is_match(line) {
                return *class;
            }
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

    #[test]
    fn chained_scorer_custom_rule_overrides_default() {
        use crate::config::{FilterRule, SignalConfig};
        let mut cfg = SignalConfig::default();
        cfg.custom_filters.push(FilterRule {
            pattern: r"DEPRECATED_API_XYZ".into(),
            class: ScoreClass::High,
            trust_tier: None,
        });
        let chained = ChainedScorer::from_config(&cfg).unwrap();
        // Default heuristics would score this Low; custom rule makes it High.
        assert_eq!(
            chained.score("call to DEPRECATED_API_XYZ()"),
            ScoreClass::High
        );
    }

    #[test]
    fn chained_scorer_without_custom_matches_default() {
        use crate::config::SignalConfig;
        let cfg = SignalConfig::default();
        let chained = ChainedScorer::from_config(&cfg).unwrap();
        assert_eq!(chained.score("Error: boom"), ScoreClass::High);
        assert_eq!(chained.score("ok"), ScoreClass::Low);
    }
}
