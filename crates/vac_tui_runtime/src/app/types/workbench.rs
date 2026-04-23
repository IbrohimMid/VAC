//! Workbench tab types, plan state, and review state.

use std::collections::HashMap;

use super::commands::{ExistingPlanPrompt, PlanComment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFocus {
    Conversation,
    Input,
    Workbench,
    Activity,
}

impl WorkspaceFocus {
    pub fn next(self) -> Self {
        match self {
            Self::Conversation => Self::Input,
            Self::Input => Self::Workbench,
            Self::Workbench => Self::Activity,
            Self::Activity => Self::Conversation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchTab {
    Approvals,
    Review,
    Sessions,
    Agents,
    Runtime,
    Plan,
    Vil,
    Vwfd,
    /// L3 — Signal workbench: list active signal streams + tail distilled
    /// view. Sources `AppState::signal_registry()`.
    Signal,
}

impl WorkbenchTab {
    pub fn next(self) -> Self {
        match self {
            Self::Approvals => Self::Review,
            Self::Review => Self::Sessions,
            Self::Sessions => Self::Agents,
            Self::Agents => Self::Runtime,
            Self::Runtime => Self::Plan,
            Self::Plan => Self::Vil,
            Self::Vil => Self::Vwfd,
            Self::Vwfd => Self::Signal,
            Self::Signal => Self::Approvals,
        }
    }
}

/// Git-review domain state. Accessed via `app_state.workspace.review`.
#[derive(Debug, Clone, Default)]
pub struct ReviewState {
    pub open: bool,
    pub filter: String,
    pub selected_idx: usize,
    pub selected_path: Option<String>,
    pub items: HashMap<String, ReviewItem>,
    pub diff: Option<ReviewDiffState>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewItemStatus {
    Pending,
    Restored,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub path: String,
    pub status: ReviewItemStatus,
    pub has_snapshot: bool,
    pub last_error: Option<String>,
    pub dirty_generation: u64,
}

#[derive(Debug, Clone)]
pub struct ReviewDiffState {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub scroll: usize,
    pub last_error: Option<String>,
}

/// Plan-mode domain state. Accessed via `app_state.workspace.plan`.
#[derive(Debug, Clone, Default)]
pub struct PlanState {
    pub mode_active: bool,
    pub metadata: Option<crate::services::plan::PlanMetadata>,
    pub draft: String,
    pub review_open: bool,
    pub review_selected: usize,
    pub review_scroll: usize,
    pub comments: Vec<PlanComment>,
    pub existing_prompt: Option<ExistingPlanPrompt>,
}
