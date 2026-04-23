//! `batch` — run a list of step-describers in one skill call.
//!
//! The skill itself doesn't execute tools — it validates the schema,
//! normalises each step to `{label, kind, args}`, and returns the
//! normalised list for the driver to dispatch. This keeps
//! `vac_skill` independent of `vac_tools`; the cost is one extra
//! hop at the call site, but the composition remains testable here.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BatchInput {
    steps: Vec<StepInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StepInput {
    #[serde(default)]
    label: Option<String>,
    kind: String,
    #[serde(default)]
    args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NormalisedStep {
    label: String,
    kind: String,
    args: serde_json::Value,
}

pub struct BatchSkill;

#[async_trait]
impl Skill for BatchSkill {
    fn name(&self) -> &str {
        "batch"
    }
    fn description(&self) -> &str {
        "Run a list of labelled steps in a single skill call. Returns the normalised list; the driver dispatches each step."
    }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["steps"],
            "properties": {
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["kind"],
                        "properties": {
                            "label": { "type": "string" },
                            "kind": { "type": "string" },
                            "args": {}
                        }
                    }
                }
            }
        })
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome> {
        let parsed: BatchInput = serde_json::from_value(ctx.input.clone())
            .map_err(|e| SkillError::InvalidInput(e.to_string()))?;
        if parsed.steps.is_empty() {
            return Err(SkillError::InvalidInput(
                "batch requires at least one step".into(),
            ));
        }
        let normalised: Vec<NormalisedStep> = parsed
            .steps
            .into_iter()
            .enumerate()
            .map(|(i, s)| NormalisedStep {
                label: s.label.unwrap_or_else(|| format!("step-{}", i + 1)),
                kind: s.kind,
                args: s.args,
            })
            .collect();
        let summary = format!("batch: {} step(s)", normalised.len());
        Ok(SkillOutcome::new(
            summary,
            json!({ "steps": normalised }),
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
    async fn normalises_steps_and_fills_missing_label() {
        let out = BatchSkill
            .run(ctx(json!({
                "steps": [
                    { "kind": "read", "args": { "path": "a.rs" } },
                    { "label": "inspect", "kind": "read", "args": { "path": "b.rs" } }
                ]
            })))
            .await
            .unwrap();
        assert_eq!(out.summary, "batch: 2 step(s)");
        let steps = out.payload["steps"].as_array().unwrap();
        assert_eq!(steps[0]["label"], "step-1");
        assert_eq!(steps[1]["label"], "inspect");
        assert_eq!(steps[0]["kind"], "read");
    }

    #[tokio::test]
    async fn empty_steps_rejected() {
        let err = BatchSkill
            .run(ctx(json!({ "steps": [] })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn missing_kind_rejected() {
        let err = BatchSkill
            .run(ctx(json!({ "steps": [{ "label": "x" }] })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }
}
