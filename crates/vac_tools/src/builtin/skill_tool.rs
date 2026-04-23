//! W3.3 — `SkillTool` dispatcher.
//!
//! The model sees exactly one entry in its tool manifest for every
//! skill — `SkillTool` — regardless of how many skills are
//! registered. Input shape: `{ "skill": <name>, "params": <obj> }`.
//!
//! Every skill invocation still routes through `vac_tools` so the
//! trust-gate, disk-spill, and transcript observers fire with
//! exactly the same semantics as any other tool call.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_skill::{SkillContext, SkillRegistry};

#[derive(Debug, Deserialize)]
struct SkillInvocation {
    skill: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// One dispatcher, wraps a shared `SkillRegistry`.
pub struct SkillTool {
    registry: Arc<SkillRegistry>,
}

impl SkillTool {
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }

    /// Describe-all surface for the CLI — re-export through the tool
    /// so callers that only have `vac_tools` don't need a direct
    /// `vac_skill` dep.
    pub fn registry(&self) -> Arc<SkillRegistry> {
        self.registry.clone()
    }
}

#[async_trait]
impl VilTool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Run a named skill. params are validated against the skill's schema."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["skill"],
            "properties": {
                "skill": { "type": "string" },
                "params": {}
            }
        })
    }

    fn trust_requirement(&self) -> &str {
        "Trusted"
    }

    fn risk_level(&self) -> &str {
        // Risk is per-skill — the aggregate classification is the
        // most-permissive stance, because a skill that writes files
        // (remember) raises risk on its own invocation path.
        "needs_approval"
    }

    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    /// W2.4 style refinement — a skill is only truly read-only when
    /// the named skill is non-mutating. Hard-coded allowlist matches
    /// the bundled skill set; new skills default to mutating and
    /// must be added here after audit.
    fn is_input_read_only(&self, input: &serde_json::Value) -> bool {
        const READ_ONLY_SKILLS: &[&str] =
            &["batch", "simplify", "stuck", "verify"];
        input
            .get("skill")
            .and_then(|s| s.as_str())
            .map(|s| READ_ONLY_SKILLS.contains(&s))
            .unwrap_or(false)
    }

    fn is_input_destructive(&self, input: &serde_json::Value) -> bool {
        !self.is_input_read_only(input)
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let invocation: SkillInvocation = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
        let ctx = SkillContext::new(
            invocation.params,
            context.working_dir.clone(),
        )
        .with_session(context.session_id);
        let ctx = match context.submit_id {
            Some(id) => ctx.with_submit(id),
            None => ctx,
        };
        let outcome = self
            .registry
            .run(&invocation.skill, ctx)
            .await
            .map_err(|e| match e {
                vac_skill::SkillError::NotFound(_) => ToolError::NotFound(e.to_string()),
                vac_skill::SkillError::InvalidInput(msg) => {
                    ToolError::InvalidArguments(msg)
                }
                other => ToolError::ExecutionFailed(other.to_string()),
            })?;
        Ok(serde_json::json!({
            "skill": invocation.skill,
            "summary": outcome.summary,
            "payload": outcome.payload,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn registry_with_bundled() -> Arc<SkillRegistry> {
        let reg = Arc::new(SkillRegistry::new());
        vac_skill::bundled::register_bundled(&reg).await.unwrap();
        reg
    }

    #[tokio::test]
    async fn skill_tool_dispatches_batch() {
        let reg = registry_with_bundled().await;
        let tool = SkillTool::new(reg);
        let ctx = ToolContext::new(std::env::temp_dir());
        let out = tool
            .execute(
                serde_json::json!({
                    "skill": "batch",
                    "params": {
                        "steps": [{ "kind": "read", "args": {} }]
                    }
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(out["skill"], "batch");
        assert!(out["summary"].as_str().unwrap().contains("batch"));
    }

    #[tokio::test]
    async fn skill_tool_missing_skill_returns_not_found() {
        let reg = registry_with_bundled().await;
        let tool = SkillTool::new(reg);
        let ctx = ToolContext::new(std::env::temp_dir());
        let err = tool
            .execute(
                serde_json::json!({ "skill": "does-not-exist" }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[tokio::test]
    async fn skill_tool_bad_args_returns_invalid() {
        let reg = registry_with_bundled().await;
        let tool = SkillTool::new(reg);
        let ctx = ToolContext::new(std::env::temp_dir());
        let err = tool
            .execute(serde_json::json!({ "wrong": "shape" }), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn is_input_read_only_honours_allowlist() {
        let reg = registry_with_bundled().await;
        let tool = SkillTool::new(reg);
        assert!(tool.is_input_read_only(&serde_json::json!({ "skill": "verify" })));
        assert!(tool.is_input_read_only(&serde_json::json!({ "skill": "simplify" })));
        assert!(!tool.is_input_read_only(&serde_json::json!({ "skill": "remember" })));
        assert!(!tool.is_input_read_only(&serde_json::json!({ "skill": "unknown" })));
    }
}
