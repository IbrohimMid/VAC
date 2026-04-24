//! B.2 — `AgentTool`: operator- or model-invokable subagent
//! dispatch.
//!
//! Builds on [`crate::subagent::SubagentRunner`] with:
//! - a model-facing input schema (subagent_type / description /
//!   prompt / optional isolation),
//! - a registry of 5 built-in subagent kinds
//!   (explore / plan / verify / general-purpose / statusline-setup;
//!    names chosen to match Claude Code's `built-in/*` so operator
//!    vocabulary is portable across ecosystems),
//! - an async dispatch function the host wires into the wider tool
//!   registry (vac_tools integration lands in a follow-up since
//!   that crate doesn't currently depend on vac_session_engine).

use serde::{Deserialize, Serialize};

use crate::subagent::{
    SubagentDispatchContext, SubagentKind, SubagentRunner, SubagentSpec,
};
use crate::error::EngineResult;

/// Input shape the LLM fills when invoking the Agent tool. Kept as
/// a narrow struct so serde deserialisation from the model's
/// `tool_use` arguments is straightforward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolInput {
    /// One of the registered subagent-type labels (see
    /// [`BUILT_IN_SUBAGENTS`]) or a custom skill identifier.
    pub subagent_type: String,
    /// Short operator-readable description of what the subagent
    /// is being asked to do. Surfaces in the Agents tab row.
    pub description: String,
    /// The prompt the subagent sees as its first user message.
    pub prompt: String,
    /// Optional isolation shape. `Some("worktree")` triggers a
    /// git-worktree fork for the subagent (Phase D.3); otherwise
    /// the subagent shares the host worktree read-only.
    #[serde(default)]
    pub isolation: Option<String>,
}

/// Metadata for one built-in subagent. Each entry lands on the
/// registry so `/agents` can list them.
#[derive(Debug, Clone, Copy)]
pub struct BuiltInAgentSpec {
    pub kind: BuiltInKind,
    /// Operator-readable title.
    pub title: &'static str,
    /// One-line description for the agents listing.
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltInKind {
    Explore,
    Plan,
    Verify,
    GeneralPurpose,
    StatuslineSetup,
}

impl BuiltInKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Plan => "plan",
            Self::Verify => "verify",
            Self::GeneralPurpose => "general-purpose",
            Self::StatuslineSetup => "statusline-setup",
        }
    }

    pub fn to_subagent_kind(self) -> SubagentKind {
        match self {
            Self::Explore => SubagentKind::Explore,
            Self::Plan => SubagentKind::Plan,
            Self::Verify => SubagentKind::Verify,
            Self::GeneralPurpose => SubagentKind::GeneralPurpose,
            Self::StatuslineSetup => SubagentKind::StatuslineSetup,
        }
    }
}

/// The five built-ins. Additional skills land via the Phase B.3
/// markdown registry and dispatch as `SubagentKind::Custom`.
pub const BUILT_IN_SUBAGENTS: &[BuiltInAgentSpec] = &[
    BuiltInAgentSpec {
        kind: BuiltInKind::Explore,
        title: "Explore",
        description: "Broad-scope research across the repo — find files, map dependencies.",
    },
    BuiltInAgentSpec {
        kind: BuiltInKind::Plan,
        title: "Plan",
        description: "Deep planning pass. Produces a step-by-step plan without executing.",
    },
    BuiltInAgentSpec {
        kind: BuiltInKind::Verify,
        title: "Verify",
        description: "Checks that an implementation matches a spec. No writes.",
    },
    BuiltInAgentSpec {
        kind: BuiltInKind::GeneralPurpose,
        title: "General purpose",
        description: "Fallback subagent for open-ended work.",
    },
    BuiltInAgentSpec {
        kind: BuiltInKind::StatuslineSetup,
        title: "Statusline setup",
        description: "Writes a statusline template to `.vac/statusline.tmpl` (Phase D.4).",
    },
];

/// Look up a built-in by label.
pub fn find_built_in(label: &str) -> Option<BuiltInAgentSpec> {
    BUILT_IN_SUBAGENTS
        .iter()
        .copied()
        .find(|s| s.kind.label() == label)
}

/// Resolve a model-supplied `subagent_type` string into a
/// `SubagentKind`. Unknown labels become `Custom(name)` so the
/// Phase B.3 skills registry can claim them.
pub fn resolve_subagent_kind(label: &str) -> SubagentKind {
    match find_built_in(label) {
        Some(spec) => spec.kind.to_subagent_kind(),
        None => SubagentKind::Custom(label.to_string()),
    }
}

/// Dispatch an AgentTool call. Returns a [`crate::SubmitStream`]
/// the caller drains; each chunk corresponds to the subagent's own
/// transcript rows. A host wrapper typically collects chunks into
/// a `ToolResultEnvelope` and feeds that back to the parent
/// submit's `tool_calls` iteration (A.3).
pub async fn dispatch_agent_tool(
    input: AgentToolInput,
    parent_session_id: uuid::Uuid,
    dispatch: SubagentDispatchContext,
) -> EngineResult<crate::SubmitStream> {
    let kind = resolve_subagent_kind(&input.subagent_type);
    let spec = SubagentSpec::new(kind, input.prompt, parent_session_id)
        .with_description(input.description);
    SubagentRunner::run(spec, dispatch).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::TrivialCompactBoundary;
    use crate::llm::EchoAdapter;
    use crate::slash::SlashProcessor;
    use crate::transcript::{TranscriptEntry, TranscriptKind, TranscriptWriter};
    use crate::usage::UsageTracker;
    use futures::StreamExt;
    use std::sync::Arc;
    use uuid::Uuid;

    #[test]
    fn built_ins_cover_five_kinds() {
        assert_eq!(BUILT_IN_SUBAGENTS.len(), 5);
        for spec in BUILT_IN_SUBAGENTS {
            assert!(find_built_in(spec.kind.label()).is_some());
        }
    }

    #[test]
    fn unknown_subagent_type_becomes_custom() {
        match resolve_subagent_kind("skill.simplify") {
            SubagentKind::Custom(name) => assert_eq!(name, "skill.simplify"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_runs_and_writes_sidechain() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = Arc::new(TranscriptWriter::new(tmp.path().to_path_buf()));
        let parent_id = Uuid::new_v4();

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

        let input = AgentToolInput {
            subagent_type: "explore".into(),
            description: "find callers of foo".into(),
            prompt: "grep foo".into(),
            isolation: None,
        };

        let mut stream = dispatch_agent_tool(input, parent_id, dispatch)
            .await
            .unwrap();
        let mut finished = false;
        while let Some(chunk) = stream.next().await {
            if matches!(chunk, crate::SubmitChunk::Finished { .. }) {
                finished = true;
            }
        }
        assert!(finished);

        let rows = writer.read(parent_id).await.unwrap();
        assert!(rows.iter().any(|r| matches!(r.kind, TranscriptKind::Sidechain)));
    }
}
