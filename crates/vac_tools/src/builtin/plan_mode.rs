//! F1.2 — `enter_plan_mode` + `exit_plan_mode` tools.
//!
//! Claude Code lesson: plan mode is a pair of tool calls, not a TUI
//! tab or overlay. Makes plan transitions uniformly traceable via the
//! existing tool-call trace/MCP pipeline. VAC keeps the
//! `WorkbenchTab::Plan` render surface but state transitions now flow
//! through tools so `vac decisions` sees them as AgentDecision records.
//!
//! Tool payload writes to `.vac/plan_mode.lock` so out-of-band
//! processes (autopilot, bridge) can observe mode transitions without
//! sharing AppState.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

const LOCK_FILE: &str = ".vac/plan_mode.lock";

#[derive(Debug, Deserialize)]
struct EnterInput {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct TransitionOutput {
    mode: &'static str,
    lock_path: String,
    reason: Option<String>,
}

pub struct EnterPlanModeTool;

impl EnterPlanModeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnterPlanModeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn description(&self) -> &str {
        "Enter plan mode — agent transitions to planning-only behaviour. No tool calls that modify state are executed until exit_plan_mode. Use when a user asks for a plan, review, or strategy discussion before action."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Optional one-line justification shown to the operator."
                }
            },
            "required": []
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: EnterInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        if let Some(r) = &input.reason {
            if r.len() > 2000 {
                return Err(ToolError::ExecutionFailed(format!(
                    "reason too long ({} > 2000 chars)",
                    r.len()
                )));
            }
        }
        let lock_path = context.working_dir.join(LOCK_FILE);
        if let Some(parent) = lock_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("create parent: {e}")))?;
        }
        let body = serde_json::json!({
            "entered_at_epoch_s": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "session_id": context.session_id.to_string(),
            "reason": input.reason,
        });
        let bytes = serde_json::to_vec_pretty(&body)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        crate::builtin::worktree::atomic_write(&lock_path, &bytes).await?;

        let out = TransitionOutput {
            mode: "plan",
            lock_path: lock_path.to_string_lossy().to_string(),
            reason: input.reason,
        };
        serde_json::to_value(out).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

pub struct ExitPlanModeTool;

impl ExitPlanModeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExitPlanModeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "exit_plan_mode"
    }

    fn description(&self) -> &str {
        "Exit plan mode — agent resumes normal tool-calling. Call after the plan is agreed and the operator wants the agent to act."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let lock_path = context.working_dir.join(LOCK_FILE);
        let removed = tokio::fs::remove_file(&lock_path).await.is_ok();
        let out = serde_json::json!({
            "mode": "execute",
            "lock_removed": removed,
            "lock_path": lock_path.to_string_lossy(),
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[tokio::test]
    async fn enter_writes_lock_file_and_exit_removes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());

        let out = EnterPlanModeTool::new()
            .execute(serde_json::json!({ "reason": "reviewing refactor" }), &ctx)
            .await
            .unwrap();
        assert_eq!(out["mode"], "plan");
        let lock = tmp.path().join(LOCK_FILE);
        assert!(lock.exists(), "entering plan mode must create lock file");

        let out = ExitPlanModeTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["mode"], "execute");
        assert_eq!(out["lock_removed"], true);
        assert!(!lock.exists(), "exit must remove lock");
    }

    #[tokio::test]
    async fn enter_without_reason_still_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = EnterPlanModeTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["mode"], "plan");
    }

    #[tokio::test]
    async fn exit_without_prior_enter_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = ExitPlanModeTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["lock_removed"], false);
    }
}
