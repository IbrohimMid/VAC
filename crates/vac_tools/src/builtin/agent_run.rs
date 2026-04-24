//! B4 — `agent_run` tool. Dispatches a subagent via the
//! `AgentDispatcher` plumbed on `ToolContext`; returns the folded
//! `ToolResultEnvelope` as JSON. Pre-B4, `agent_list` only exposed
//! metadata — the LLM could see the subagent kinds but not
//! actually invoke them. This closes that loop.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::AgentDispatchInput;

#[derive(Debug, Deserialize)]
struct Input {
    /// Subagent kind: "explore" | "plan" | "verify" | "general-purpose" | "statusline-setup" | custom.
    subagent_type: String,
    /// Short description (operator-readable, surfaces in UI rows).
    description: String,
    /// Prompt the subagent sees as its first user message.
    prompt: String,
    /// Optional isolation shape. `"worktree"` triggers a git
    /// worktree fork; default is shared worktree.
    #[serde(default)]
    isolation: Option<String>,
}

pub struct AgentRunTool;

impl AgentRunTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentRunTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for AgentRunTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "agent_run"
    }

    fn description(&self) -> &str {
        "Dispatch a subagent (explore / plan / verify / general-purpose / statusline-setup / custom skill id). Returns the subagent's content + tool_calls folded into a single envelope. Requires a live session dispatcher (unavailable in minimal / plan-only modes)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "subagent_type": { "type": "string" },
                "description": { "type": "string" },
                "prompt": { "type": "string" },
                "isolation": { "type": "string", "enum": ["worktree"] }
            },
            "required": ["subagent_type", "description", "prompt"]
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
        // ADR-002 runtime guard — deep delegation trees require
        // a formal model (quota/gate/approval inheritance, cancel
        // tree, transcript tree) VAC v1 has not designed. Reject
        // nested `agent_run` with an explicit pointer to the ADR
        // so operators can restructure their workflow.
        if context.depth >= 1 {
            return Err(ToolError::ExecutionFailed(
                "Nested subagents are not supported in VAC v1. First-level \
                 delegation only — see docs/adr/ADR-002-subagent-depth-policy.md. \
                 Restructure so the parent agent (depth 0) dispatches all \
                 subagents directly rather than through another subagent."
                    .into(),
            ));
        }
        let input: Input = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let dispatcher = context.agent_dispatcher.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed(
                "agent_run: no AgentDispatcher on ToolContext. This tool \
                 requires a live session (vac run / TUI) — it is not \
                 available in minimal / plan-only mode."
                    .into(),
            )
        })?;
        let envelope = dispatcher
            .dispatch(
                AgentDispatchInput {
                    subagent_type: input.subagent_type,
                    description: input.description,
                    prompt: input.prompt,
                    isolation: input.isolation,
                },
                context.session_id,
            )
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("dispatch: {e}")))?;
        serde_json::to_value(envelope)
            .map_err(|e| ToolError::ExecutionFailed(format!("serialize: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[tokio::test]
    async fn agent_run_errors_when_no_dispatcher() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let err = AgentRunTool::new()
            .execute(
                serde_json::json!({
                    "subagent_type": "explore",
                    "description": "x",
                    "prompt": "y"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no AgentDispatcher"));
    }

    /// ADR-002 — nested `agent_run` is hard-denied.
    #[tokio::test]
    async fn agent_run_denies_nested_depth() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        ctx.depth = 1; // subagent ctx
        let err = AgentRunTool::new()
            .execute(
                serde_json::json!({
                    "subagent_type": "explore",
                    "description": "x",
                    "prompt": "y"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Nested subagents are not supported"), "{msg}");
        assert!(msg.contains("ADR-002"), "error should cite the ADR: {msg}");
    }

    /// ADR-002 — deeper nesting (depth 2+) also denied.
    #[tokio::test]
    async fn agent_run_denies_deeply_nested_depth() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        ctx.depth = 5;
        let err = AgentRunTool::new()
            .execute(
                serde_json::json!({
                    "subagent_type": "plan",
                    "description": "x",
                    "prompt": "y"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("ADR-002"));
    }

    #[tokio::test]
    async fn agent_run_dispatches_when_present() {
        use std::sync::Arc;
        use std::pin::Pin;
        use std::future::Future;
        use vac_session_primitives::{AgentDispatcher, EngineResult};
        use vac_tool_core::{ToolResultEnvelope, ToolResultKind};

        struct StubDispatcher;
        impl AgentDispatcher for StubDispatcher {
            fn dispatch<'a>(
                &'a self,
                input: AgentDispatchInput,
                _parent: uuid::Uuid,
            ) -> Pin<Box<dyn Future<Output = EngineResult<ToolResultEnvelope>> + Send + 'a>>
            {
                Box::pin(async move {
                    Ok(ToolResultEnvelope {
                        kind: ToolResultKind::Ok,
                        summary: format!("stub {}", input.subagent_type),
                        payload: serde_json::json!({ "stub": true }),
                        duration_ms: 0,
                    })
                })
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        ctx.agent_dispatcher = Some(Arc::new(StubDispatcher));
        let out = AgentRunTool::new()
            .execute(
                serde_json::json!({
                    "subagent_type": "explore",
                    "description": "x",
                    "prompt": "y"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(out["kind"], "ok");
        assert_eq!(out["payload"]["stub"], true);
    }
}
