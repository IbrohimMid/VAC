//! F1.4 — `schedule_cron` tool.
//!
//! Claude Code lesson: cron schedules are created via a tool call, not
//! a separate subcommand. Agents can schedule their own follow-ups
//! (e.g. "re-run VIL audit daily at 9am") inline.
//!
//! Writes an entry to `.vac/autopilot.schedules.toml` (separate from
//! the main autopilot config so operator edits don't collide). The
//! vac_runtime cron runner picks up entries from both locations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

const SCHEDULES_FILE: &str = ".vac/autopilot.schedules.toml";

#[derive(Debug, Deserialize)]
struct Input {
    /// Schedule identifier (must be unique).
    id: String,
    /// Cron expression (5-field minute-hour-dom-month-dow).
    cron: String,
    /// Task description — used as the prompt when the schedule fires.
    task: String,
    /// Optional rulebook ID to apply when the scheduled task runs.
    #[serde(default)]
    rulebook: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ScheduleEntry {
    id: String,
    cron: String,
    task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rulebook: Option<String>,
    created_at_epoch_s: u64,
    created_by_session: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ScheduleDoc {
    #[serde(default)]
    schedules: Vec<ScheduleEntry>,
}

pub struct ScheduleCronTool;

impl ScheduleCronTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ScheduleCronTool {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_cron(expr: &str) -> Result<(), String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!(
            "cron expression must have 5 fields (minute hour dom month dow), got {}",
            fields.len()
        ));
    }
    Ok(())
}

#[async_trait]
impl VilTool for ScheduleCronTool {
    fn name(&self) -> &str {
        "schedule_cron"
    }

    fn description(&self) -> &str {
        "Register a cron-scheduled task that autopilot will invoke at the specified times. Use for recurring maintenance (daily VIL audits, hourly linters) without blocking the current session."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique schedule identifier." },
                "cron": { "type": "string", "description": "5-field cron expression (minute hour dom month dow)." },
                "task": { "type": "string", "description": "Prompt the agent runs when the schedule fires." },
                "rulebook": { "type": "string", "description": "Optional rulebook ID applied to the scheduled task." }
            },
            "required": ["id", "cron", "task"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "ask_once"
    }

    fn risk_level(&self) -> &str {
        "mutating"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: Input = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        if input.id.is_empty() || input.id.len() > 64 {
            return Err(ToolError::ExecutionFailed(
                "schedule id must be 1..=64 chars".into(),
            ));
        }
        if !input
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ToolError::ExecutionFailed(
                "schedule id must be [A-Za-z0-9_-]+".into(),
            ));
        }
        if input.task.len() > 8192 {
            return Err(ToolError::ExecutionFailed(
                "task prompt too long (max 8192 chars)".into(),
            ));
        }
        validate_cron(&input.cron)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid cron: {e}")))?;

        let path = context.working_dir.join(SCHEDULES_FILE);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("create parent: {e}")))?;
        }

        let mut doc: ScheduleDoc = match tokio::fs::read_to_string(&path).await {
            Ok(content) => toml::from_str(&content)
                .map_err(|e| ToolError::ExecutionFailed(format!("parse schedules: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ScheduleDoc::default(),
            Err(e) => return Err(ToolError::ExecutionFailed(format!("read schedules: {e}"))),
        };

        if doc.schedules.iter().any(|s| s.id == input.id) {
            return Err(ToolError::ExecutionFailed(format!(
                "schedule id '{}' already exists; remove it first",
                input.id
            )));
        }

        let entry = ScheduleEntry {
            id: input.id.clone(),
            cron: input.cron.clone(),
            task: input.task.clone(),
            rulebook: input.rulebook.clone(),
            created_at_epoch_s: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            created_by_session: context.session_id.to_string(),
        };
        doc.schedules.push(entry);

        let serialized = toml::to_string_pretty(&doc)
            .map_err(|e| ToolError::ExecutionFailed(format!("serialize: {e}")))?;
        crate::builtin::worktree::atomic_write(&path, serialized.as_bytes()).await?;

        let out = serde_json::json!({
            "id": input.id,
            "cron": input.cron,
            "schedules_file": path.to_string_lossy(),
            "total_schedules": doc.schedules.len(),
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[test]
    fn validate_cron_accepts_5_fields() {
        assert!(validate_cron("0 9 * * *").is_ok());
        assert!(validate_cron("*/5 * * * *").is_ok());
    }

    #[test]
    fn validate_cron_rejects_wrong_field_count() {
        assert!(validate_cron("0 9 *").is_err());
        assert!(validate_cron("0 9 * * * *").is_err());
        assert!(validate_cron("").is_err());
    }

    #[tokio::test]
    async fn schedule_writes_toml_and_increments_count() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let tool = ScheduleCronTool::new();

        let out = tool
            .execute(
                serde_json::json!({
                    "id": "daily-audit",
                    "cron": "0 9 * * *",
                    "task": "run vil audit",
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(out["total_schedules"], 1);

        let out = tool
            .execute(
                serde_json::json!({
                    "id": "hourly-lint",
                    "cron": "0 * * * *",
                    "task": "cargo clippy",
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(out["total_schedules"], 2);

        let content = std::fs::read_to_string(tmp.path().join(SCHEDULES_FILE)).unwrap();
        assert!(content.contains("daily-audit"));
        assert!(content.contains("hourly-lint"));
    }

    #[tokio::test]
    async fn duplicate_id_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let tool = ScheduleCronTool::new();
        let args = serde_json::json!({
            "id": "same",
            "cron": "0 9 * * *",
            "task": "x",
        });
        tool.execute(args.clone(), &ctx).await.unwrap();
        let err = tool.execute(args, &ctx).await.unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }
}
