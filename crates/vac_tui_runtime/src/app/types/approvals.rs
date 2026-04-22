use std::collections::HashMap;

use crate::types::ToolCall;

/// Approval domain state grouped out of `AppState`.
#[derive(Debug, Clone, Default)]
pub struct ApprovalsState {
    pub pending_approvals: Vec<ToolCall>,
    pub pending_tool_calls: Vec<ToolCall>,
    pub approved_tools: Vec<ToolCall>,
    pub rejected_tools: Vec<ToolCall>,
    pub approval_selected_idx: usize,
    pub approval_detail_scroll: usize,
    pub approval_explanations: HashMap<String, Option<String>>,
    pub reject_reason_input: Option<String>,
}
