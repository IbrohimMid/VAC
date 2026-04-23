//! Token usage, billing, and session information types.

#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BillingInfo {
    pub credits_remaining: Option<u64>,
    pub tier: Option<String>,
}

/// R1.a — token/context/billing/auth grouping pulled out of AppState.
/// `auth_display` is a (identity, plan, credits) tuple mirroring the
/// header's three-slot layout.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct BillingState {
    pub current_message: TokenUsage,
    pub total_session: TokenUsage,
    pub context_usage_percent: f32,
    pub billing_info: Option<BillingInfo>,
    pub auth_display: (Option<String>, Option<String>, Option<String>),
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub title: String,
    pub id: String,
    pub updated_at: String,
    pub checkpoints: Vec<String>,
    pub task_count: usize,
    pub last_activity: String,
    pub has_checkpoint: bool,
    pub snapshot_present: bool,
    pub snapshot_stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LoadingOperation {
    LlmRequest,
    ToolExecution,
    SessionsList,
    StreamProcessing,
    CheckpointResume,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Approved,
    Rejected,
    Executed,
    Skipped,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShortcutsPopupMode {
    #[default]
    Commands,
    Shortcuts,
    Sessions,
}

#[derive(Debug)]
pub struct LoadingStateManager {
    active_operations: std::collections::HashSet<LoadingOperation>,
}

impl Default for LoadingStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadingStateManager {
    pub fn new() -> Self {
        Self {
            active_operations: std::collections::HashSet::new(),
        }
    }

    pub fn start_operation(&mut self, operation: LoadingOperation) {
        self.active_operations.insert(operation);
    }

    pub fn end_operation(&mut self, operation: LoadingOperation) {
        self.active_operations.remove(&operation);
    }

    pub fn is_loading(&self) -> bool {
        !self.active_operations.is_empty()
    }

    pub fn clear_all(&mut self) {
        self.active_operations.clear();
    }
}
