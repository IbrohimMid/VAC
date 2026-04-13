//! AgentRunState — consolidated mutable state for a single agent loop execution.
//! Extracted from orchestrator.rs to formalize the control-plane state contract.

use crate::events::EventCollector;
use vil_llm::provider::{Message, ToolCall};
use vac_tools::approvals::PendingApproval;

/// Consolidated mutable state for a single agent loop execution.
///
/// Every field that `execute_agent_loop` previously scattered across separate
/// `&mut` parameters now lives here. This makes the pipeline auditable:
/// callers can inspect the full run state at any step boundary.
pub struct AgentRunState {
    /// Conversation messages (system + user + assistant + tool).
    pub messages: Vec<Message>,
    /// Trim boundary index — messages before this index have been context-reduced.
    /// Monotonically increasing across turns for cache stability.
    pub trim_boundary: usize,
    /// Cumulative token usage across all LLM calls in this run.
    pub total_tokens: u64,
    /// Files modified by tool calls during this run.
    pub modified_files: Vec<String>,
    /// Files created by tool calls during this run.
    pub created_files: Vec<String>,
    /// Number of completed agent loop iterations.
    pub iterations: usize,
    /// Structured event collector for observability/audit.
    pub collector: EventCollector,
    /// Cancellation token — checked at each pipeline step boundary.
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    /// Current stage marker for checkpoint metadata.
    pub stage: RunStage,
    /// Active tool calls pending execution (for checkpoint restore).
    pub active_tool_calls: Vec<ToolCall>,
    /// Tool calls awaiting human approval (for checkpoint restore).
    pub pending_approvals: Vec<PendingApproval>,
    /// Last execution status message (for checkpoint restore).
    pub last_execution_status: Option<String>,
    /// Store for trimmed message content (for cache-preserving context reduction).
    pub trim_store: crate::context_budget::TrimStore,
}

/// Which high-level stage the agent loop is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RunStage {
    /// Semantic planner is running.
    Planner,
    /// Coder agent is running.
    Coder,
    /// Run completed successfully.
    Completed,
    /// Run was cancelled.
    Cancelled,
    /// Run failed.
    Failed,
}

impl std::fmt::Display for RunStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planner => write!(f, "planner"),
            Self::Coder => write!(f, "coder"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl AgentRunState {
    /// Create a new run state for the given initial messages.
    pub fn new(
        messages: Vec<Message>,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> Self {
        Self {
            messages,
            trim_boundary: 0,
            total_tokens: 0,
            modified_files: Vec::new(),
            created_files: Vec::new(),
            iterations: 0,
            collector: EventCollector::new(),
            cancel,
            stage: RunStage::Planner,
            active_tool_calls: Vec::new(),
            pending_approvals: Vec::new(),
            last_execution_status: None,
            trim_store: crate::context_budget::TrimStore::default(),
        }
    }

    /// Check if the run has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.is_cancelled())
    }

    /// Save checkpoint to file.
    pub fn save_checkpoint(&self, path: &std::path::Path, run_id: Option<uuid::Uuid>) -> Result<(), crate::checkpoint::CheckpointError> {
        let metadata = serde_json::json!({
            "stage": self.stage.to_string(),
            "iterations": self.iterations,
            "total_tokens": self.total_tokens,
            "trim_boundary": self.trim_boundary,
            "modified_files": self.modified_files,
            "created_files": self.created_files,
            "active_tool_calls": self.active_tool_calls,
            "pending_approvals": self.pending_approvals,
            "last_execution_status": self.last_execution_status,
            "trim_store": serde_json::to_value(&self.trim_store.0).unwrap_or_default(),
        });
        let envelope = crate::checkpoint::CheckpointEnvelope::new(run_id, self.messages.clone(), metadata);
        crate::checkpoint::save_checkpoint_to_file(path, &envelope)
    }

    /// Load checkpoint from file and restore state.
    pub fn from_checkpoint(path: &std::path::Path) -> Result<Self, crate::checkpoint::CheckpointError> {
        let envelope = crate::checkpoint::load_checkpoint_from_file(path)?;
        let Some(metadata) = envelope.metadata.as_object() else {
            return Err(crate::checkpoint::CheckpointError::InvalidPayload(
                serde_json::from_str::<serde_json::Value>("{}").unwrap_err()
            ));
        };

        let stage = metadata.get("stage")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "planner" => Some(RunStage::Planner),
                "coder" => Some(RunStage::Coder),
                "completed" => Some(RunStage::Completed),
                "cancelled" => Some(RunStage::Cancelled),
                "failed" => Some(RunStage::Failed),
                _ => None,
            })
            .unwrap_or(RunStage::Planner);

        Ok(Self {
            messages: envelope.messages,
            trim_boundary: metadata.get("trim_boundary").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            total_tokens: metadata.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            iterations: metadata.get("iterations").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            modified_files: metadata.get("modified_files")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            created_files: metadata.get("created_files")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            stage,
            cancel: None,
            collector: EventCollector::new(),
            active_tool_calls: metadata.get("active_tool_calls")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            pending_approvals: metadata.get("pending_approvals")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            last_execution_status: metadata.get("last_execution_status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            trim_store: crate::context_budget::TrimStore(
                metadata.get("trim_store")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default()
            ),
        })
    }

    /// Record a file modification (deduplicating).
    pub fn record_modified(&mut self, path: String) {
        if !self.modified_files.contains(&path) {
            self.modified_files.push(path);
        }
    }

    /// Record a file creation (deduplicating).
    pub fn record_created(&mut self, path: String) {
        if !self.created_files.contains(&path) {
            self.created_files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vil_llm::provider::Message;

    #[test]
    fn new_state_has_sensible_defaults() {
        let state = AgentRunState::new(
            vec![Message::system("sys".to_string()), Message::user("task".to_string())],
            None,
        );
        assert_eq!(state.trim_boundary, 0);
        assert_eq!(state.total_tokens, 0);
        assert_eq!(state.iterations, 0);
        assert!(state.modified_files.is_empty());
        assert!(state.created_files.is_empty());
        assert_eq!(state.stage, RunStage::Planner);
        assert!(!state.is_cancelled());
    }

    #[test]
    fn record_file_deduplicates() {
        let mut state = AgentRunState::new(vec![], None);
        state.record_modified("src/main.rs".to_string());
        state.record_modified("src/main.rs".to_string());
        state.record_created("src/new.rs".to_string());
        state.record_created("src/new.rs".to_string());
        assert_eq!(state.modified_files.len(), 1);
        assert_eq!(state.created_files.len(), 1);
    }

    #[test]
    fn cancellation_detected() {
        let token = tokio_util::sync::CancellationToken::new();
        let state = AgentRunState::new(vec![], Some(token.clone()));
        assert!(!state.is_cancelled());
        token.cancel();
        assert!(state.is_cancelled());
    }

    #[test]
    fn stage_display() {
        assert_eq!(RunStage::Planner.to_string(), "planner");
        assert_eq!(RunStage::Coder.to_string(), "coder");
        assert_eq!(RunStage::Completed.to_string(), "completed");
        assert_eq!(RunStage::Cancelled.to_string(), "cancelled");
        assert_eq!(RunStage::Failed.to_string(), "failed");
    }

    #[test]
    fn checkpoint_roundtrip() {
        let mut state = AgentRunState::new(
            vec![Message::user("test task".to_string())],
            None,
        );
        state.iterations = 5;
        state.total_tokens = 1000;
        state.record_modified("src/main.rs".to_string());
        state.stage = RunStage::Coder;

        let temp_path = std::env::temp_dir().join("test_checkpoint.json");
        state.save_checkpoint(&temp_path, None).unwrap();

        let restored = AgentRunState::from_checkpoint(&temp_path).unwrap();
        assert_eq!(restored.iterations, 5);
        assert_eq!(restored.total_tokens, 1000);
        assert_eq!(restored.modified_files.len(), 1);
        assert_eq!(restored.stage, RunStage::Coder);
        assert_eq!(restored.messages[0].content, "test task");

        std::fs::remove_file(temp_path).ok();
    }
}
