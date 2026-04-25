//! D4 / D4.1 — runtime event → activity projection.
//!
//! The shell's `ActivityLog` consumes `ShellActivityEntry`. Live
//! VAC engines emit richer events (`RuntimeUpdate`,
//! `AgentLoopEvent`, `vac_session_engine::stream::SubmitChunk`,
//! …). To avoid pulling those types into the shell stack, hosts
//! adapt them to a neutral [`RuntimeEventView`] DTO; this crate
//! projects from there into `ShellActivityEntry`.
//!
//! The projection is intentionally lossy: only the fields the
//! activity stream needs (kind, severity, title, optional detail)
//! travel. Anything richer should land in the engine-side trace,
//! not the operator-visible activity log.
//!
//! # Boundary
//!
//! Allowed deps: `vac_shell_contracts`, `vac_shell_host_activity`,
//! `serde`. No `vac_core`, no `vac_session_engine`, no
//! `vac_tui_runtime`, no donor crates.

use serde::{Deserialize, Serialize};
use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityKind};
use vac_shell_host_activity::ActivityLog;

/// Neutral runtime-event DTO. Hosts adapt their concrete engine
/// events into this shape; everything else flows through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeEventView {
    UserInput {
        id: String,
        ts_unix: u64,
        text: String,
    },
    AgentThoughtSummary {
        id: String,
        ts_unix: u64,
        summary: String,
    },
    ToolStarted {
        id: String,
        ts_unix: u64,
        name: String,
        args_summary: Option<String>,
    },
    ToolFinished {
        id: String,
        ts_unix: u64,
        name: String,
        ok: bool,
        summary: Option<String>,
    },
    FileEdited {
        id: String,
        ts_unix: u64,
        path: String,
    },
    ShellCommand {
        id: String,
        ts_unix: u64,
        command: String,
    },
    ApprovalRequested {
        id: String,
        ts_unix: u64,
        tool: String,
    },
    ApprovalResolved {
        id: String,
        ts_unix: u64,
        tool: String,
        approved: bool,
    },
    ModelChanged {
        id: String,
        ts_unix: u64,
        provider: String,
        model_id: String,
    },
    Error {
        id: String,
        ts_unix: u64,
        title: String,
        detail: Option<String>,
    },
}

impl RuntimeEventView {
    pub fn id(&self) -> &str {
        match self {
            RuntimeEventView::UserInput { id, .. }
            | RuntimeEventView::AgentThoughtSummary { id, .. }
            | RuntimeEventView::ToolStarted { id, .. }
            | RuntimeEventView::ToolFinished { id, .. }
            | RuntimeEventView::FileEdited { id, .. }
            | RuntimeEventView::ShellCommand { id, .. }
            | RuntimeEventView::ApprovalRequested { id, .. }
            | RuntimeEventView::ApprovalResolved { id, .. }
            | RuntimeEventView::ModelChanged { id, .. }
            | RuntimeEventView::Error { id, .. } => id,
        }
    }

    pub fn ts_unix(&self) -> u64 {
        match self {
            RuntimeEventView::UserInput { ts_unix, .. }
            | RuntimeEventView::AgentThoughtSummary { ts_unix, .. }
            | RuntimeEventView::ToolStarted { ts_unix, .. }
            | RuntimeEventView::ToolFinished { ts_unix, .. }
            | RuntimeEventView::FileEdited { ts_unix, .. }
            | RuntimeEventView::ShellCommand { ts_unix, .. }
            | RuntimeEventView::ApprovalRequested { ts_unix, .. }
            | RuntimeEventView::ApprovalResolved { ts_unix, .. }
            | RuntimeEventView::ModelChanged { ts_unix, .. }
            | RuntimeEventView::Error { ts_unix, .. } => *ts_unix,
        }
    }
}

/// Project one runtime event into the activity-log entry shape.
pub fn project_runtime_event(event: RuntimeEventView) -> ShellActivityEntry {
    match event {
        RuntimeEventView::UserInput { id, ts_unix, text } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::UserInput,
            title: text,
            detail: None,
            severity: Severity::Info,
        },
        RuntimeEventView::AgentThoughtSummary {
            id,
            ts_unix,
            summary,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::AgentThoughtSummary,
            title: summary,
            detail: None,
            severity: Severity::Info,
        },
        RuntimeEventView::ToolStarted {
            id,
            ts_unix,
            name,
            args_summary,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ToolCall,
            title: name,
            detail: args_summary,
            severity: Severity::Info,
        },
        RuntimeEventView::ToolFinished {
            id,
            ts_unix,
            name,
            ok,
            summary,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ToolResult,
            title: name,
            detail: summary,
            severity: if ok { Severity::Ok } else { Severity::Error },
        },
        RuntimeEventView::FileEdited { id, ts_unix, path } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::FileEdit,
            title: path,
            detail: None,
            severity: Severity::Info,
        },
        RuntimeEventView::ShellCommand {
            id,
            ts_unix,
            command,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ShellCommand,
            title: command,
            detail: None,
            severity: Severity::Info,
        },
        RuntimeEventView::ApprovalRequested { id, ts_unix, tool } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ApprovalRequested,
            title: tool,
            detail: None,
            severity: Severity::Warn,
        },
        RuntimeEventView::ApprovalResolved {
            id,
            ts_unix,
            tool,
            approved,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ApprovalResolved,
            title: tool,
            detail: Some(if approved { "approved".into() } else { "rejected".into() }),
            severity: if approved { Severity::Ok } else { Severity::Warn },
        },
        RuntimeEventView::ModelChanged {
            id,
            ts_unix,
            provider,
            model_id,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ModelChanged,
            title: format!("{provider} / {model_id}"),
            detail: None,
            severity: Severity::Ok,
        },
        RuntimeEventView::Error {
            id,
            ts_unix,
            title,
            detail,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::Error,
            title,
            detail,
            severity: Severity::Error,
        },
    }
}

// =====================================================================
// D4.1 — ingest helper
// =====================================================================

/// Project + record one event into the supplied activity log.
pub fn record_projected_event(log: &ActivityLog, event: RuntimeEventView) {
    log.record(project_runtime_event(event));
}
