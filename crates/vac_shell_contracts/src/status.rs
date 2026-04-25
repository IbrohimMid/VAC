//! Slice 19 — global status bar DTO.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShellStatusView {
    /// Display name for the active surface (e.g. "chat", "runtime").
    /// String rather than enum so future surfaces don't widen the
    /// contract.
    pub surface: Option<String>,
    pub model_label: Option<String>,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub pending_approvals: usize,
    pub running_tasks: usize,
    pub last_error: Option<String>,
}
