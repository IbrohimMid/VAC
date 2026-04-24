//! NS.1 — `hook_list` + `hook_delete` tools. Read/mutate
//! `.vac/hooks.json` via `vac_session_primitives::HookStore`.
//!
//! Admin-gated per the NS.1 plan: exposing `HookCommand::Command`
//! registration to the LLM is an RCE vector until the sandbox
//! lands in Part 3 / NS.4. `hook_create` is deliberately NOT
//! shipped in Part 1 — operators add hooks by editing hooks.json
//! directly. List + delete are safe (read-only + destructive-only
//! with no code execution).

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::HookStore;

// ── hook_list ────────────────────────────────────────────────

pub struct HookListTool;

impl HookListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HookListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for HookListTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "hook_list"
    }

    fn description(&self) -> &str {
        "List hooks registered in .vac/hooks.json. Read-only. Returns each hook's id, event, matcher, and command kind. Use before hook_delete to confirm the target id."
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
        let store = HookStore::load(&context.working_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("load: {e}")))?;
        Ok(serde_json::json!({
            "entries": store.entries,
            "count": store.entries.len(),
        }))
    }
}

// ── hook_delete ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeleteInput {
    id: String,
}

pub struct HookDeleteTool;

impl HookDeleteTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HookDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for HookDeleteTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "hook_delete"
    }

    fn description(&self) -> &str {
        "Remove a hook from .vac/hooks.json by id. Destructive but does not execute any hook commands."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Hook id to remove." }
            },
            "required": ["id"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "privileged"
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
        let mut store = HookStore::load(&context.working_dir)
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
    use vac_session_primitives::hooks::{HookCommand, HookEntry, HookEvent};

    fn entry(id: &str) -> HookEntry {
        HookEntry {
            id: id.into(),
            event: HookEvent::PreToolUse,
            matcher: "Edit".into(),
            command: HookCommand::Command { argv: vec!["true".into()] },
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn list_empty_store() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = HookListTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["count"], 0);
    }

    #[tokio::test]
    async fn delete_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = HookStore::default();
        store.create(entry("h1")).unwrap();
        store.save(tmp.path()).await.unwrap();

        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = HookDeleteTool::new()
            .execute(serde_json::json!({"id": "h1"}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["removed"], true);
        assert_eq!(out["remaining"], 0);
    }
}
