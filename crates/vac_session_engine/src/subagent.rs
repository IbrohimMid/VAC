//! B.1 — `SubagentRunner`: operator/model-invokable subagents.
//!
//! A subagent is an isolated conversation: fresh session id, fresh
//! InitialMessages, own abort handle, own MCP registry (conceptually —
//! today that means a fresh transcript file + a fresh submit stream;
//! richer isolation lands with Phase B.2's AgentTool). The runner
//! produces a [`SubmitStream`] the parent caller drains while writing
//! one `TranscriptKind::Sidechain` row in the parent transcript so
//! time-travel (`/thinkback`) can replay subagent activity as part
//! of the larger session.
//!
//! This primitive is the foundation Phase B.2 (`AgentTool`) builds
//! on. It can also be called directly by CLI bridges or by the hook
//! registry's `agent` hook type (Phase C.5).

use std::sync::Arc;
use uuid::Uuid;

use crate::compact::CompactBoundary;
use crate::llm::LlmAdapter;
use crate::slash::SlashProcessor;
use crate::stream::{SubmitStream, submit_stream};
use crate::submit::CompactConfig;
use crate::transcript::{TranscriptEntry, TranscriptKind, TranscriptWriter};
use crate::usage::UsageTracker;
use crate::event::SubmitContext;

/// Recognised subagent kinds. `Custom(name)` allows drivers to
/// register arbitrary identifiers (skills, markdown-defined
/// agents) without extending the enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentKind {
    /// Broad-scope research / exploration.
    Explore,
    /// Deep planning pass, often offloaded to a stronger model.
    Plan,
    /// Verifier — asserts an implementation matches a spec.
    Verify,
    /// Generic fallback.
    GeneralPurpose,
    /// Helper that writes a status line template.
    StatuslineSetup,
    /// Registered by the Skills registry (B.3).
    Custom(String),
}

impl SubagentKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Explore => "explore",
            Self::Plan => "plan",
            Self::Verify => "verify",
            Self::GeneralPurpose => "general-purpose",
            Self::StatuslineSetup => "statusline-setup",
            Self::Custom(name) => name.as_str(),
        }
    }

    pub fn from_label(s: &str) -> Self {
        match s {
            "explore" | "Explore" => Self::Explore,
            "plan" | "Plan" => Self::Plan,
            "verify" | "verification" | "Verify" => Self::Verify,
            "general-purpose" | "general_purpose" => Self::GeneralPurpose,
            "statusline-setup" | "statusline_setup" => Self::StatuslineSetup,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Spec the runner expands into a subagent submit.
#[derive(Debug, Clone)]
pub struct SubagentSpec {
    pub kind: SubagentKind,
    pub prompt: String,
    /// Parent session id — used to link the Sidechain transcript
    /// row back. The subagent's own session id is generated
    /// internally.
    pub parent_session_id: Uuid,
    /// Short operator-readable description — surfaces in the
    /// Agents workbench tab and in activity rows.
    pub description: Option<String>,
}

impl SubagentSpec {
    pub fn new(
        kind: SubagentKind,
        prompt: impl Into<String>,
        parent_session_id: Uuid,
    ) -> Self {
        Self {
            kind,
            prompt: prompt.into(),
            parent_session_id,
            description: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Dispatch helper bundle so callers don't have to rewire all
/// eight arguments each time they spawn a subagent.
#[derive(Clone)]
pub struct SubagentDispatchContext {
    pub transcript: Arc<TranscriptWriter>,
    pub slash: Arc<SlashProcessor>,
    pub compact: Arc<dyn CompactBoundary>,
    pub usage: Arc<UsageTracker>,
    pub llm: Arc<dyn LlmAdapter>,
    pub compact_cfg: CompactConfig,
}

impl SubagentDispatchContext {
    pub fn new(
        transcript: Arc<TranscriptWriter>,
        slash: Arc<SlashProcessor>,
        compact: Arc<dyn CompactBoundary>,
        usage: Arc<UsageTracker>,
        llm: Arc<dyn LlmAdapter>,
    ) -> Self {
        Self {
            transcript,
            slash,
            compact,
            usage,
            llm,
            compact_cfg: CompactConfig::default(),
        }
    }

    pub fn with_compact_cfg(mut self, cfg: CompactConfig) -> Self {
        self.compact_cfg = cfg;
        self
    }
}

/// `SubagentRunner` materialises a subagent as a [`SubmitStream`].
/// Emits one Sidechain transcript row on the *parent* session
/// before returning so the parent transcript always has a pointer
/// to the subagent's run.
pub struct SubagentRunner;

impl SubagentRunner {
    /// Run a subagent. Returns the stream the caller drains; any
    /// chunks on that stream correspond to the subagent's own
    /// session (fresh session id, fresh transcript). Dropping the
    /// stream aborts the underlying submit (same semantics as
    /// dropping a `submit_stream` result).
    pub async fn run(
        spec: SubagentSpec,
        ctx: SubagentDispatchContext,
    ) -> crate::error::EngineResult<SubmitStream> {
        let subagent_session = Uuid::new_v4();

        // Sidechain breadcrumb on the parent transcript.
        let parent_handle = ctx
            .transcript
            .open(spec.parent_session_id)
            .await?;
        let row = TranscriptEntry::new(
            spec.parent_session_id,
            TranscriptKind::Sidechain,
            serde_json::json!({
                "subagent_session": subagent_session,
                "subagent_type": spec.kind.label(),
                "prompt": spec.prompt,
                "description": spec.description,
            }),
        );
        ctx.transcript.append(&parent_handle, &row).await?;

        // Subagent submit — fresh session, same transcript writer
        // so its jsonl lives alongside the parent's.
        let child_ctx = SubmitContext::new(subagent_session, spec.prompt.clone())
            .with_metadata(serde_json::json!({
                "subagent_type": spec.kind.label(),
                "parent_session": spec.parent_session_id,
            }));

        Ok(submit_stream(
            child_ctx,
            ctx.transcript.clone(),
            ctx.slash.clone(),
            ctx.compact.clone(),
            ctx.usage.clone(),
            ctx.llm.clone(),
            ctx.compact_cfg.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::TrivialCompactBoundary;
    use crate::llm::EchoAdapter;
    use futures::StreamExt;

    #[tokio::test]
    async fn subagent_runs_and_writes_sidechain_row() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = Arc::new(TranscriptWriter::new(tmp.path().to_path_buf()));
        let parent_id = Uuid::new_v4();

        // Seed a parent transcript so append() has a handle.
        let parent_handle = writer.open(parent_id).await.unwrap();
        let seed = TranscriptEntry::new(
            parent_id,
            TranscriptKind::Accepted,
            serde_json::json!({ "input": "parent" }),
        );
        writer.append(&parent_handle, &seed).await.unwrap();

        let dispatch = SubagentDispatchContext::new(
            writer.clone(),
            Arc::new(SlashProcessor::new()),
            Arc::new(TrivialCompactBoundary::default()),
            Arc::new(UsageTracker::new()),
            Arc::new(EchoAdapter),
        );
        let spec = SubagentSpec::new(
            SubagentKind::Explore,
            "find files mentioning foo",
            parent_id,
        )
        .with_description("test sidechain");

        let mut stream = SubagentRunner::run(spec, dispatch).await.unwrap();
        let mut saw_finished = false;
        while let Some(chunk) = stream.next().await {
            if matches!(chunk, crate::stream::SubmitChunk::Finished { .. }) {
                saw_finished = true;
            }
        }
        assert!(saw_finished, "subagent stream must close with Finished");

        let parent_rows = writer.read(parent_id).await.unwrap();
        assert!(
            parent_rows
                .iter()
                .any(|r| matches!(r.kind, TranscriptKind::Sidechain)),
            "parent transcript must carry a Sidechain row for the subagent",
        );
    }

    #[test]
    fn kind_roundtrips_through_label() {
        for k in [
            SubagentKind::Explore,
            SubagentKind::Plan,
            SubagentKind::Verify,
            SubagentKind::GeneralPurpose,
            SubagentKind::StatuslineSetup,
            SubagentKind::Custom("skill.simplify".into()),
        ] {
            let label = k.label().to_string();
            let back = SubagentKind::from_label(&label);
            assert_eq!(k, back);
        }
    }
}
