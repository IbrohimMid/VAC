use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What the caller submits to the engine. Minimal — drivers layer on
/// their own context (VIL project profile, rulebook, model override)
/// via the submit pipeline, not via this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitContext {
    /// Stable session identifier. Transcript rows group by this.
    pub session_id: Uuid,
    /// Raw operator/agent input. `slash::SlashProcessor` decides
    /// whether this becomes a user message or triggers a command.
    pub input: String,
    /// Wall-clock timestamp of submission (rfc3339).
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    /// Optional metadata the driver wants to carry through (model
    /// override, rulebook id, bridge origin). Serialized to the
    /// transcript as-is.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl SubmitContext {
    pub fn new(session_id: Uuid, input: impl Into<String>) -> Self {
        Self {
            session_id,
            input: input.into(),
            submitted_at: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Events the engine emits during a submit lifecycle. UIs/CLI surface
/// them differently (TUI renders, CLI prints, bridge forwards) but
/// every driver sees the same stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SubmitEvent {
    /// Submit accepted and stub transcript entry has been persisted.
    /// Downstream tools or the LLM have not been contacted yet. This
    /// is the durability checkpoint: if the process dies here, the
    /// next session boot can see the pending submit and resume.
    Accepted { entry_id: Uuid },

    /// A slash command was identified and handled locally — no LLM
    /// round-trip. Payload carries whatever the slash handler returned
    /// for the UI to display.
    SlashHandled {
        command: String,
        payload: serde_json::Value,
    },

    /// Compact boundary fired — some messages were pruned/summarised
    /// before LLM contact. Downstream UIs may render a dimmed
    /// "summarised N turns" line.
    Compacted { kept: usize, dropped: usize },

    /// LLM request was issued. Driver wires provider/model metadata.
    LlmRequested {
        provider: String,
        model: String,
    },

    /// LLM streamed a chunk of assistant text.
    LlmChunk { text: String },

    /// LLM emitted a tool call request.
    ToolRequested {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },

    /// Tool executed, result ready.
    ToolResult {
        id: String,
        name: String,
        payload: vac_tool_core::ToolResultEnvelope,
    },

    /// Submit finished cleanly. `usage` carries final token/cost
    /// totals. Transcript now has a Finished row.
    Finished { usage: crate::usage::UsageSnapshot },

    /// Submit aborted (cancel, policy denial, error).
    Aborted { reason: String },

    /// Planner predicted the next task. Emitted after `Finished`.
    SpeculationReady {
        predicted_prompt: String,
        precomputed_context: std::collections::HashMap<String, String>,
    },
}

impl SubmitEvent {
    /// Short operator-facing label, for ticker-style UIs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Accepted { .. } => "accepted",
            Self::SlashHandled { .. } => "slash",
            Self::Compacted { .. } => "compact",
            Self::LlmRequested { .. } => "llm.request",
            Self::LlmChunk { .. } => "llm.chunk",
            Self::ToolRequested { .. } => "tool.request",
            Self::ToolResult { .. } => "tool.result",
            Self::Finished { .. } => "finished",
            Self::Aborted { .. } => "aborted",
            Self::SpeculationReady { .. } => "speculation_ready",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_context_new_defaults_metadata_null() {
        let c = SubmitContext::new(Uuid::new_v4(), "hello");
        assert_eq!(c.input, "hello");
        assert!(c.metadata.is_null());
    }

    #[test]
    fn submit_event_label_stable() {
        assert_eq!(
            SubmitEvent::LlmChunk { text: "x".into() }.label(),
            "llm.chunk"
        );
        assert_eq!(
            SubmitEvent::Accepted {
                entry_id: Uuid::new_v4()
            }
            .label(),
            "accepted"
        );
    }

    #[test]
    fn submit_event_roundtrips_through_json() {
        let ev = SubmitEvent::ToolRequested {
            id: "tc-1".into(),
            name: "file_read".into(),
            arguments: serde_json::json!({ "path": "Cargo.toml" }),
        };
        let s = serde_json::to_string(&ev).unwrap();
        let back: SubmitEvent = serde_json::from_str(&s).unwrap();
        match back {
            SubmitEvent::ToolRequested { name, .. } => assert_eq!(name, "file_read"),
            _ => panic!("wrong variant"),
        }
    }
}
