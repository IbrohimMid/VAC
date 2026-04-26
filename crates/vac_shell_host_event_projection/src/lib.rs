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
//! `serde`. **D10 exception (fifth ADR-sanctioned host-side exception)**:
//! `vac_session_engine` (for `SubmitStream` / `SubmitChunk`) and
//! `vac_tool_core` (for `ToolResultEnvelope`) are added to support
//! `spawn_activity_feed_bridge`. This crate is not reachable from any
//! widget / bridge / app / entrypoint / runtime-loop runtime graph.

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
        /// D10-HARDENING: severity derived from ToolResultKind (Ok→Ok,
        /// Warning/Cancelled→Warn, Error→Error). Replaces the old `ok: bool`
        /// that collapsed Warning and Cancelled into Error.
        severity: Severity,
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
            severity,
            summary,
        } => ShellActivityEntry {
            id,
            ts_unix,
            kind: ShellActivityKind::ToolResult,
            title: name,
            detail: summary,
            severity,
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

// =====================================================================
// D10 — live activity feed bridge
// =====================================================================

fn current_ts_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// D10 — spawn a background task that drains `stream` and records each
/// meaningful chunk as a `ShellActivityEntry` in `log`.
///
/// Chunk → `RuntimeEventView` mapping:
/// - `TextDelta / Accepted / SlashHandled / Compacted / SpeculationReady` → skipped
/// - `LlmRequested` → `AgentThoughtSummary`
/// - `ToolRequested` → `ToolStarted`
/// - `ToolResult` → `ToolFinished`
/// - `Finished` → `AgentThoughtSummary` with token counts
/// - `Aborted` → `Error`
///
/// Returns a `JoinHandle` — caller can drop it (fire-and-forget) or
/// `.await` it after the submit completes. The task ends when the
/// stream is exhausted.
pub fn spawn_activity_feed_bridge(
    stream: vac_session_engine::stream::SubmitStream,
    log: ActivityLog,
    session_id: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use futures::StreamExt as _;
        use vac_session_engine::stream::SubmitChunk;
        use vac_tool_core::ToolResultKind;

        let mut stream = stream;
        let mut seq: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let ts = current_ts_secs();
            seq += 1;
            let id = format!("bridge-{session_id}-{seq}");

            let event: Option<RuntimeEventView> = match chunk {
                SubmitChunk::TextDelta { .. }
                | SubmitChunk::Accepted { .. }
                | SubmitChunk::SlashHandled { .. }
                | SubmitChunk::Compacted { .. }
                | SubmitChunk::SpeculationReady { .. } => None,

                SubmitChunk::LlmRequested { provider, model } => {
                    Some(RuntimeEventView::AgentThoughtSummary {
                        id,
                        ts_unix: ts,
                        summary: format!("contacting {provider} / {model}"),
                    })
                }

                SubmitChunk::ToolRequested { id: tool_id, name, .. } => {
                    // D10-HARDENING: arguments are never forwarded to the
                    // activity log — they may contain secrets. The tool name
                    // alone is sufficient for the operator activity stream.
                    Some(RuntimeEventView::ToolStarted {
                        id: format!("bridge-{session_id}-tool-{tool_id}"),
                        ts_unix: ts,
                        name,
                        args_summary: None,
                    })
                }

                SubmitChunk::ToolResult { id: tool_id, name, payload } => {
                    // D10-HARDENING: map ToolResultKind faithfully.
                    // Warning and Cancelled → Severity::Warn (not Error).
                    let severity = match payload.kind {
                        ToolResultKind::Ok => Severity::Ok,
                        ToolResultKind::Warning | ToolResultKind::Cancelled => Severity::Warn,
                        ToolResultKind::Error => Severity::Error,
                    };
                    let summary = if payload.summary.is_empty() {
                        None
                    } else {
                        Some(payload.summary.clone())
                    };
                    Some(RuntimeEventView::ToolFinished {
                        id: format!("bridge-{session_id}-result-{tool_id}"),
                        ts_unix: ts,
                        name,
                        severity,
                        summary,
                    })
                }

                SubmitChunk::Finished { usage } => {
                    Some(RuntimeEventView::AgentThoughtSummary {
                        id,
                        ts_unix: ts,
                        summary: format!(
                            "submit finished — {} in / {} out tokens",
                            usage.input_tokens, usage.output_tokens
                        ),
                    })
                }

                SubmitChunk::Aborted { reason } => Some(RuntimeEventView::Error {
                    id,
                    ts_unix: ts,
                    title: "submit aborted".into(),
                    detail: Some(reason),
                }),

                // Future SubmitChunk variants — skip silently.
                _ => None,
            };

            if let Some(ev) = event {
                record_projected_event(&log, ev);
            }
        }
    })
}
