//! NS.1 + NS.4 — `hook_list` / `hook_delete` / `hook_create` tools.
//! Read/mutate `.vac/hooks.json` via `vac_session_primitives`.
//!
//! `hook_create` ships now that NS.4's `HookSandbox` enforces a
//! restrictive env allowlist + rlimits + wall-clock cap on every
//! `HookCommand::Command` invocation through `HookGate`. Without
//! the sandbox this would be an RCE vector; with it, the worst a
//! malicious hook can do is chew the 10-second CPU cap on its own
//! process and return Deny. Schema validation on load (`validate_
//! hook_store`) rejects malformed entries before they reach the
//! execution path.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::{HookCommand, HookEntry, HookEvent, HookStore, validate_hook_store};

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

// ── hook_create ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateInput {
    id: String,
    event: String,
    #[serde(default)]
    matcher: String,
    /// One of: `command` (argv), `prompt` (string), `agent` (kind+prompt), `http` (url).
    kind: String,
    #[serde(default)]
    argv: Vec<String>,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    agent_kind: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

fn parse_event(s: &str) -> Result<HookEvent, ToolError> {
    Ok(match s {
        "PreToolUse" => HookEvent::PreToolUse,
        "PostToolUse" => HookEvent::PostToolUse,
        "UserPromptSubmit" => HookEvent::UserPromptSubmit,
        "Stop" => HookEvent::Stop,
        "SubagentStop" => HookEvent::SubagentStop,
        "Notification" => HookEvent::Notification,
        "SessionStart" => HookEvent::SessionStart,
        "SessionEnd" => HookEvent::SessionEnd,
        "PreCompact" => HookEvent::PreCompact,
        other => {
            return Err(ToolError::ExecutionFailed(format!(
                "unknown hook event '{other}'; expected one of PreToolUse/PostToolUse/UserPromptSubmit/Stop/SubagentStop/Notification/SessionStart/SessionEnd/PreCompact"
            )));
        }
    })
}

fn build_command(input: &CreateInput) -> Result<HookCommand, ToolError> {
    match input.kind.as_str() {
        "command" => {
            if input.argv.is_empty() {
                return Err(ToolError::ExecutionFailed(
                    "kind=command requires non-empty argv".into(),
                ));
            }
            Ok(HookCommand::Command { argv: input.argv.clone() })
        }
        "prompt" => Ok(HookCommand::Prompt { prompt: input.prompt.clone() }),
        "agent" => Ok(HookCommand::Agent {
            kind: input.agent_kind.clone(),
            prompt: input.prompt.clone(),
        }),
        "http" => Ok(HookCommand::Http { url: input.url.clone() }),
        other => Err(ToolError::ExecutionFailed(format!(
            "unknown hook kind '{other}'; expected command/prompt/agent/http"
        ))),
    }
}

pub struct HookCreateTool;

impl HookCreateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HookCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for HookCreateTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "hook_create"
    }

    fn description(&self) -> &str {
        "Register a hook in .vac/hooks.json. Shell hooks (kind=command) execute under HookSandbox: env allowlist, 512 MB / 10s CPU / 30s wall rlimits. Schema-validated at load."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "event": {
                    "type": "string",
                    "enum": ["PreToolUse", "PostToolUse", "UserPromptSubmit", "Stop", "SubagentStop", "Notification", "SessionStart", "SessionEnd", "PreCompact"]
                },
                "matcher": { "type": "string", "description": "Regex against tool name (empty = match all)." },
                "kind": { "type": "string", "enum": ["command", "prompt", "agent", "http"] },
                "argv": { "type": "array", "items": { "type": "string" }, "description": "Required when kind=command." },
                "prompt": { "type": "string", "description": "Required when kind=prompt or agent." },
                "agent_kind": { "type": "string", "description": "Required when kind=agent." },
                "url": { "type": "string", "description": "Required when kind=http." },
                "description": { "type": "string" }
            },
            "required": ["id", "event", "kind"]
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
        let input: CreateInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let entry = HookEntry {
            id: input.id.clone(),
            event: parse_event(&input.event)?,
            matcher: input.matcher.clone(),
            command: build_command(&input)?,
            description: input.description.clone(),
        };
        let mut store = HookStore::load(&context.working_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("load: {e}")))?;
        store
            .create(entry)
            .map_err(|e| ToolError::ExecutionFailed(format!("create: {e}")))?;
        // Revalidate the whole store so a newly-added regex collision
        // or id-charset violation surfaces at call time, not 6h later
        // when the hook fires.
        validate_hook_store(&store)
            .map_err(|e| ToolError::ExecutionFailed(format!("validate: {e}")))?;
        store
            .save(&context.working_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("save: {e}")))?;
        Ok(serde_json::json!({
            "id": input.id,
            "total": store.entries.len(),
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
