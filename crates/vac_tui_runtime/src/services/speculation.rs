//! P3 — next-submit predictor.
//!
//! Minimum-viable: takes the last operator submit + the prompt
//! history ring, infers a plausible next prompt via a small set of
//! heuristics, and warms a context map. Pluggable via the
//! `NextSubmitPredictor` trait so a later implementation can swap in
//! a real LLM-backed planner without changing the caller.
//!
//! Driver wires this after `SubmitEvent::Finished`:
//!
//! ```ignore
//! let prediction = predictor.predict(&last_submit, history).await;
//! if let Some(p) = prediction {
//!     state.speculation.set_predicted(p.prompt, p.context);
//! }
//! ```

use std::collections::HashMap;

use async_trait::async_trait;

use crate::services::prompt_suggest::PromptHistory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prediction {
    pub prompt: String,
    pub context: HashMap<String, String>,
}

#[async_trait]
pub trait NextSubmitPredictor: Send + Sync {
    async fn predict(
        &self,
        last_submit: &str,
        history: &PromptHistory,
    ) -> Option<Prediction>;
}

/// Heuristic predictor — no LLM. Rules:
///
/// 1. If the last submit matches `/^(write|add)\s+/`, predict
///    `"run the tests for <target>"` as the natural follow-up.
/// 2. If the last submit matches `/fix/i`, predict
///    `"explain the root cause of the issue we just fixed"`.
/// 3. Otherwise: return the most-recent-but-different prompt from
///    the history ring. This is pattern-matching "the operator does
///    the same kind of task again" which is surprisingly common for
///    code-review + refactor cycles.
///
/// Deterministic + cheap; good baseline before a real predictor
/// lands. Returns `None` when none of the rules trigger.
#[derive(Debug, Default, Clone)]
pub struct HeuristicPredictor;

#[async_trait]
impl NextSubmitPredictor for HeuristicPredictor {
    async fn predict(
        &self,
        last_submit: &str,
        history: &PromptHistory,
    ) -> Option<Prediction> {
        let trimmed = last_submit.trim();
        if trimmed.is_empty() {
            return None;
        }
        let lower = trimmed.to_ascii_lowercase();

        // Rule 1: write/add → run the tests
        if lower.starts_with("write ") || lower.starts_with("add ") {
            let target = trimmed
                .splitn(2, char::is_whitespace)
                .nth(1)
                .unwrap_or(trimmed);
            let mut ctx = HashMap::new();
            ctx.insert(
                "hint".into(),
                "Typical follow-up after write/add is to exercise the change.".into(),
            );
            return Some(Prediction {
                prompt: format!("run the tests for {}", target),
                context: ctx,
            });
        }

        // Rule 2: fix → explain root cause
        if lower.contains("fix") {
            let mut ctx = HashMap::new();
            ctx.insert(
                "hint".into(),
                "Root-cause write-up helps the next incident.".into(),
            );
            return Some(Prediction {
                prompt: "explain the root cause of the issue we just fixed".into(),
                context: ctx,
            });
        }

        // Rule 3: next-most-recent-but-different from history.
        // Walk the history from newest → oldest, skip anything
        // matching the current submit.
        for prev in history.suggest("", 8) {
            if prev != trimmed {
                let mut ctx = HashMap::new();
                ctx.insert(
                    "hint".into(),
                    "Operators often re-run a similar kind of task.".into(),
                );
                return Some(Prediction {
                    prompt: prev,
                    context: ctx,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_rule_predicts_run_tests() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        let out = p.predict("write unit tests for auth", &h).await.unwrap();
        assert!(out.prompt.starts_with("run the tests"));
        assert!(out.context.contains_key("hint"));
    }

    #[tokio::test]
    async fn fix_rule_predicts_root_cause() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        let out = p.predict("fix the panic in session engine", &h).await.unwrap();
        assert!(out.prompt.contains("root cause"));
    }

    #[tokio::test]
    async fn history_fallback_picks_different_prior_prompt() {
        let p = HeuristicPredictor;
        let mut h = PromptHistory::default();
        h.record("refactor module A");
        h.record("audit tests");
        let out = p.predict("audit tests", &h).await.unwrap();
        assert_eq!(out.prompt, "refactor module A");
    }

    #[tokio::test]
    async fn empty_submit_returns_none() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        assert!(p.predict("   ", &h).await.is_none());
    }

    #[tokio::test]
    async fn no_heuristic_no_history_returns_none() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        assert!(p.predict("some random submit", &h).await.is_none());
    }
}
