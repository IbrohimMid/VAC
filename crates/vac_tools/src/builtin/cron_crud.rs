//! NS.1 — `cron_list` + `cron_delete` tools. Read/mutate the
//! canonical cron store at `.vac/cron.json` via
//! `vac_session_primitives::CronStore`. Create is covered by the
//! existing `schedule_cron` tool (writes a separate TOML file
//! under `.vac/autopilot.schedules.toml` for autopilot-specific
//! schedules). These two tools operate on the session-engine
//! cron store that the runtime loop polls every 30 s.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::CronStore;

// ── cron_list ────────────────────────────────────────────────

pub struct CronListTool;

impl CronListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CronListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for CronListTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "cron_list"
    }

    fn description(&self) -> &str {
        "List cron-scheduled tasks from .vac/cron.json (session-engine store). Returns each entry's id, schedule, prompt, last_fire_unix, fire_count."
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
        let store = CronStore::load(&context.working_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("load cron store: {e}")))?;
        Ok(serde_json::json!({
            "entries": store.entries,
            "count": store.entries.len(),
        }))
    }
}

// ── cron_delete ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeleteInput {
    id: String,
}

pub struct CronDeleteTool;

impl CronDeleteTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CronDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for CronDeleteTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "cron_delete"
    }

    fn description(&self) -> &str {
        "Remove a cron-scheduled task by id from .vac/cron.json. Returns whether an entry was actually removed."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Cron entry id to remove." }
            },
            "required": ["id"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "ask_once"
    }

    fn risk_level(&self) -> &str {
        "destructive"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: DeleteInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let mut store = CronStore::load(&context.working_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("load: {e}")))?;
        let removed = store.delete(&input.id);
        if removed {
            store
                .save(&context.working_dir)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("save: {e}")))?;
        }
        Ok(serde_json::json!({
            "id": input.id,
            "removed": removed,
            "remaining": store.entries.len(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;
    use vac_session_primitives::CronEntry;

    fn sample(id: &str) -> CronEntry {
        CronEntry {
            id: id.into(),
            name: format!("t-{id}"),
            schedule: "0 * * * * *".into(),
            prompt: "p".into(),
            subagent_type: "explore".into(),
            created_at_unix: 1_700_000_000,
            last_fire_unix: 0,
            fire_count: 0,
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn list_returns_empty_on_missing_store() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = CronListTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["count"], 0);
    }

    #[tokio::test]
    async fn delete_removes_existing_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = CronStore::default();
        store.create(sample("a")).unwrap();
        store.save(tmp.path()).await.unwrap();

        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = CronDeleteTool::new()
            .execute(serde_json::json!({"id": "a"}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["removed"], true);
        assert_eq!(out["remaining"], 0);
    }

    #[tokio::test]
    async fn delete_reports_false_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = CronDeleteTool::new()
            .execute(serde_json::json!({"id": "nope"}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["removed"], false);
    }
}
