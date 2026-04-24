//! B4 — `AgentDispatcher` trait.
//!
//! `vac_tools` wants to invoke subagent dispatch from an
//! LLM-callable tool, but can't depend on `vac_session_engine`
//! without re-introducing the cycle Part 1 broke. This trait
//! lives in `vac_session_primitives` (leaf, in both graphs). The
//! concrete impl wraps `vac_session_engine::dispatch_agent_tool`.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vac_tool_core::ToolResultEnvelope;

use crate::error::EngineResult;

/// Plain-data subagent invocation input. Mirrors
/// `vac_session_engine::AgentToolInput` so the tool wrapper can
/// deserialize once and pass the struct across the trait boundary
/// without re-introducing an engine-side dep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDispatchInput {
    /// Subagent kind label — one of "explore", "plan", "verify",
    /// "general-purpose", "statusline-setup" — or a custom skill id.
    pub subagent_type: String,
    /// Operator-readable description that surfaces in the UI.
    pub description: String,
    /// Prompt the subagent sees as its first user message.
    pub prompt: String,
    /// Optional isolation shape. `Some("worktree")` triggers a
    /// git worktree fork for the subagent.
    #[serde(default)]
    pub isolation: Option<String>,
}

/// Trait the `agent_run` tool calls to spawn a subagent. The
/// concrete impl (in `vac_session_engine`) drains the subagent's
/// `SubmitStream`, folds tool results + final text into a single
/// `ToolResultEnvelope`, and hands the envelope back to the tool
/// wrapper for the parent submit's tool-result row.
pub trait AgentDispatcher: Send + Sync {
    /// Dispatch a subagent under the given parent session.
    /// Returns `ToolResultEnvelope` so the tool wrapper can fold
    /// directly into its `execute` return value — no stream
    /// plumbing escapes into `vac_tools`.
    fn dispatch<'a>(
        &'a self,
        input: AgentDispatchInput,
        parent_session_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = EngineResult<ToolResultEnvelope>> + Send + 'a>>;
}
