//! `stuck` — detect agent loop and emit an escalation payload.
//!
//! Given a short history of recent tool calls (names only), flag when
//! the agent has called the same tool ≥ `threshold` times in a row
//! without meaningful progress. The skill doesn't execute anything;
//! it emits `{ stuck: bool, reason, advice }` so the driver decides
//! how to break out (ask user, re-plan, stop).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

pub const DEFAULT_THRESHOLD: usize = 3;
pub const MAX_HISTORY: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StuckInput {
    recent_tools: Vec<String>,
    #[serde(default)]
    threshold: Option<usize>,
}

pub(crate) fn classify(recent: &[String], threshold: usize) -> (bool, String) {
    if recent.is_empty() {
        return (false, "no tool history".into());
    }
    let tail = recent.last().unwrap();
    let run = recent
        .iter()
        .rev()
        .take_while(|n| *n == tail)
        .count();
    if run >= threshold {
        (
            true,
            format!(
                "{tool} called {run}x in a row (threshold {threshold})",
                tool = tail,
                run = run,
                threshold = threshold,
            ),
        )
    } else {
        (
            false,
            format!(
                "current run={run} under threshold {threshold}",
                run = run,
                threshold = threshold,
            ),
        )
    }
}

pub struct StuckSkill;

#[async_trait]
impl Skill for StuckSkill {
    fn name(&self) -> &str {
        "stuck"
    }
    fn description(&self) -> &str {
        "Detect agent loop from recent tool-call history and emit an escalation payload."
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["recent_tools"],
            "properties": {
                "recent_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "maxItems": MAX_HISTORY
                },
                "threshold": { "type": "integer", "minimum": 2 }
            }
        })
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome> {
        let parsed: StuckInput = serde_json::from_value(ctx.input.clone())
            .map_err(|e| SkillError::InvalidInput(e.to_string()))?;
        if parsed.recent_tools.len() > MAX_HISTORY {
            return Err(SkillError::InvalidInput(format!(
                "recent_tools must be <= {MAX_HISTORY} entries"
            )));
        }
        let threshold = parsed.threshold.unwrap_or(DEFAULT_THRESHOLD).max(2);
        let (stuck, reason) = classify(&parsed.recent_tools, threshold);
        let advice = if stuck {
            "Break the loop: re-plan with a different tool, ask the operator, or narrow the prompt."
        } else {
            "Continue — no stuck pattern detected."
        };
        Ok(SkillOutcome::new(
            if stuck {
                format!("stuck: {reason}")
            } else {
                "not stuck".into()
            },
            json!({
                "stuck": stuck,
                "reason": reason,
                "advice": advice,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx(input: serde_json::Value) -> SkillContext {
        SkillContext::new(input, PathBuf::from("."))
    }

    #[test]
    fn classify_fires_on_repeated_run() {
        let hist = vec!["grep".to_string(); 3];
        let (stuck, reason) = classify(&hist, 3);
        assert!(stuck);
        assert!(reason.contains("grep"));
    }

    #[test]
    fn classify_quiet_when_under_threshold() {
        let hist = vec!["grep".into(), "grep".into()];
        let (stuck, _) = classify(&hist, 3);
        assert!(!stuck);
    }

    #[test]
    fn classify_counts_run_from_end_only() {
        // Same name earlier in history shouldn't inflate the run.
        let hist = vec![
            "grep".into(),
            "read".into(),
            "grep".into(),
            "grep".into(),
        ];
        let (stuck, _) = classify(&hist, 3);
        assert!(!stuck, "run from tail is 2, below threshold 3");
    }

    #[test]
    fn classify_empty_history() {
        let (stuck, _) = classify(&[], 3);
        assert!(!stuck);
    }

    #[tokio::test]
    async fn stuck_skill_flags_loop_via_default_threshold() {
        let out = StuckSkill
            .run(ctx(json!({
                "recent_tools": ["grep", "grep", "grep"]
            })))
            .await
            .unwrap();
        assert_eq!(out.payload["stuck"], true);
    }

    #[tokio::test]
    async fn stuck_skill_respects_custom_threshold() {
        let out = StuckSkill
            .run(ctx(json!({
                "recent_tools": ["grep", "grep"],
                "threshold": 2
            })))
            .await
            .unwrap();
        assert_eq!(out.payload["stuck"], true);
    }

    #[tokio::test]
    async fn stuck_skill_rejects_oversize_history() {
        let big: Vec<String> = (0..MAX_HISTORY + 1).map(|i| format!("t{i}")).collect();
        let err = StuckSkill
            .run(ctx(json!({ "recent_tools": big })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }
}
