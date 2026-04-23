//! P3 — next-submit predictor.
//!
//! `Planner::predict_next_submit` is the hot path the engine adapter
//! calls after `SubmitEvent::Finished` to warm a likely follow-up.
//! We keep it deterministic + cheap (no LLM, no I/O) so a bogus
//! prediction never stalls the post-submit path.
//!
//! Rules (first match wins):
//!
//! 1. `write …` / `add …` → "run the tests for <target>".
//!    Rationale: after authoring new code the operator almost always
//!    runs the test suite.
//! 2. `fix …` (anywhere in the submit) → "explain the root cause of
//!    the issue we just fixed".
//!    Rationale: fix commits want post-mortem write-ups.
//! 3. `refactor …` → "verify the refactor preserved behavior".
//!    Rationale: operators run targeted tests after a refactor.
//! 4. Fallback: `"Follow up on: <summary>"`. Keeps the legacy
//!    behavior so callers that relied on the old string still work.

use crate::error::SwarmError;

pub struct Planner;

impl Planner {
    pub async fn predict_next_submit(
        last_submit_summary: &str,
    ) -> Result<String, SwarmError> {
        let trimmed = last_submit_summary.trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower.starts_with("write ") || lower.starts_with("add ") {
            let target = trimmed
                .splitn(2, char::is_whitespace)
                .nth(1)
                .unwrap_or(trimmed);
            return Ok(format!("run the tests for {}", target));
        }
        if lower.contains("fix") {
            return Ok("explain the root cause of the issue we just fixed".into());
        }
        if lower.starts_with("refactor ") {
            return Ok("verify the refactor preserved behavior".into());
        }
        Ok(format!("Follow up on: {}", trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_rule_predicts_tests() {
        let out = Planner::predict_next_submit("write auth module").await.unwrap();
        assert_eq!(out, "run the tests for auth module");
    }

    #[tokio::test]
    async fn add_rule_predicts_tests() {
        let out = Planner::predict_next_submit("add retry logic").await.unwrap();
        assert!(out.starts_with("run the tests for"));
    }

    #[tokio::test]
    async fn fix_rule_predicts_root_cause() {
        let out = Planner::predict_next_submit("fix the panic in session engine")
            .await
            .unwrap();
        assert!(out.contains("root cause"));
    }

    #[tokio::test]
    async fn refactor_rule_predicts_verify() {
        let out = Planner::predict_next_submit("refactor the transcript writer")
            .await
            .unwrap();
        assert_eq!(out, "verify the refactor preserved behavior");
    }

    #[tokio::test]
    async fn fallback_prefixes_summary() {
        let out = Planner::predict_next_submit("review the PR").await.unwrap();
        assert_eq!(out, "Follow up on: review the PR");
    }
}
