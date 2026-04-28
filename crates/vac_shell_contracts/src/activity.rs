//! Slice 14 — activity-stream DTOs the host crate populates from a
//! VAC event bus and the widget renders.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ShellActivityFilter {
    #[default]
    All,
    Errors,
    Warnings,
    Status,
    Diagnostics,
    Tools,
    Approvals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShellActivityKind {
    UserInput,
    AgentThoughtSummary,
    ToolCall,
    ToolResult,
    Diagnostic,
    Status,
    FileEdit,
    ShellCommand,
    ApprovalRequested,
    ApprovalResolved,
    ModelChanged,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellActivityEntry {
    pub id: String,
    pub ts_unix: u64,
    pub kind: ShellActivityKind,
    pub title: String,
    pub detail: Option<String>,
    pub severity: Severity,
}
