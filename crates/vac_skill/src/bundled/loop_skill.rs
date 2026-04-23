//! `loop` — re-enter the same skill or prompt at a cadence.
//!
//! The skill takes an `interval_secs`, `max_iterations`, and a
//! nested `action` (a skill invocation). It *describes* a loop —
//! the driver owns actual scheduling via `vac_runtime::cron_scheduler`
//! — but this module validates input and emits a manifest the
//! scheduler can ingest without a second parse.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

/// Upper bound on `interval_secs` so a typo doesn't schedule a loop
/// that fires every millisecond. 1 hour maximum keeps the skill
/// grounded; callers who need longer should use explicit cron.
pub const MAX_INTERVAL_SECS: u64 = 3_600;
/// Upper bound on `max_iterations` to avoid runaway loops.
pub const MAX_ITERATIONS: u64 = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoopInput {
    interval_secs: u64,
    #[serde(default)]
    max_iterations: Option<u64>,
    action: serde_json::Value,
}

pub struct LoopSkill;

#[async_trait]
impl Skill for LoopSkill {
    fn name(&self) -> &str {
        "loop"
    }
    fn description(&self) -> &str {
        "Schedule a recurring skill invocation at a fixed interval. Returns a manifest the scheduler consumes."
    }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["interval_secs", "action"],
            "properties": {
                "interval_secs": { "type": "integer", "minimum": 1, "maximum": MAX_INTERVAL_SECS },
                "max_iterations": { "type": "integer", "minimum": 1, "maximum": MAX_ITERATIONS },
                "action": { "type": "object" }
            }
        })
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome> {
        let parsed: LoopInput = serde_json::from_value(ctx.input.clone())
            .map_err(|e| SkillError::InvalidInput(e.to_string()))?;
        if parsed.interval_secs == 0 || parsed.interval_secs > MAX_INTERVAL_SECS {
            return Err(SkillError::InvalidInput(format!(
                "interval_secs must be in 1..={MAX_INTERVAL_SECS}"
            )));
        }
        if let Some(n) = parsed.max_iterations {
            if n == 0 || n > MAX_ITERATIONS {
                return Err(SkillError::InvalidInput(format!(
                    "max_iterations must be in 1..={MAX_ITERATIONS}"
                )));
            }
        }
        if !parsed.action.is_object() {
            return Err(SkillError::InvalidInput("action must be an object".into()));
        }
        let summary = match parsed.max_iterations {
            Some(n) => format!(
                "loop: every {}s, up to {n} iteration(s)",
                parsed.interval_secs
            ),
            None => format!("loop: every {}s, open-ended", parsed.interval_secs),
        };
        Ok(SkillOutcome::new(
            summary,
            json!({
                "manifest": {
                    "interval_secs": parsed.interval_secs,
                    "max_iterations": parsed.max_iterations,
                    "action": parsed.action,
                }
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

    #[tokio::test]
    async fn happy_open_ended_loop() {
        let out = LoopSkill
            .run(ctx(json!({
                "interval_secs": 30,
                "action": { "skill": "verify" }
            })))
            .await
            .unwrap();
        assert!(out.summary.contains("open-ended"));
        assert_eq!(out.payload["manifest"]["interval_secs"], 30);
    }

    #[tokio::test]
    async fn bounded_loop_has_iterations_in_summary() {
        let out = LoopSkill
            .run(ctx(json!({
                "interval_secs": 60,
                "max_iterations": 5,
                "action": { "skill": "verify" }
            })))
            .await
            .unwrap();
        assert!(out.summary.contains("5 iteration"));
    }

    #[tokio::test]
    async fn zero_interval_rejected() {
        let err = LoopSkill
            .run(ctx(json!({
                "interval_secs": 0,
                "action": { "x": 1 }
            })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn oversized_interval_rejected() {
        let err = LoopSkill
            .run(ctx(json!({
                "interval_secs": MAX_INTERVAL_SECS + 1,
                "action": { "x": 1 }
            })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn non_object_action_rejected() {
        let err = LoopSkill
            .run(ctx(json!({
                "interval_secs": 10,
                "action": "string"
            })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }
}
