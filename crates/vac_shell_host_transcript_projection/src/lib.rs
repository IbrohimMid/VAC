//! D9 — operator-safe projection of D7E/D8 transcript
//! tool-use rows into [`ShellActivityEntry`] and a small
//! [`ToolUseActivitySummary`] suitable for the session
//! browser tile.
//!
//! # Boundary
//!
//! ```text
//! UI / widget / bridge / app / runtime-loop / entrypoint
//!   ──/──> NO normal-dep edge to this crate
//!
//! vac_shell_host_transcript_projection
//!   ├── vac_session_engine        (read_tool_use_rows + ToolUseTranscriptView)
//!   ├── vac_shell_contracts       (Severity, ShellActivityKind, ShellActivityEntry)
//!   └── vac_tool_core             (ToolResultEnvelope, ToolResultKind)
//! ```
//!
//! # Read-only by design
//!
//! Every public function reads the JSONL file at the supplied
//! path. No row is appended, rewritten, or deleted. The
//! transcript remains the source of truth.
//!
//! # Operator redaction contract
//!
//! Projected activity rows expose **only** these fields for
//! every tool call:
//!
//! * tool name (the LLM-emitted `name`)
//! * status (`ok` / `warning` / `error` / `cancelled` /
//!   `pending` when the result row is missing)
//! * envelope `summary` (1–2 lines, operator-readable, written
//!   by the dispatcher — never the model)
//! * `duration_ms`
//! * transcript path
//!
//! The raw `envelope.payload`, the original `arguments`, and
//! any subsequent transcript rows are deliberately **not**
//! rendered. They remain in the transcript file on disk for
//! anyone with shell access; the cockpit projection never
//! displays them. Tests in this crate pin the redaction.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vac_session_engine::{ToolUseReplayError, ToolUseTranscriptView, read_tool_use_rows};
use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityKind, ToolUseUiStatus};
use vac_tool_core::ToolResultKind;

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("transcript replay error: {0}")]
    Replay(#[from] ToolUseReplayError),
}

/// Status the projection assigns to each tool-use entry.
/// Mirrors `ToolResultKind` plus a `Pending` value used when
/// the transcript records a `tool_call` row but no matching
/// `tool_result` row (mid-flight crash, truncated session,
/// pre-D7E transcript that never got a result).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolUseStatus {
    Ok,
    Warning,
    Error,
    Cancelled,
    Pending,
}

impl ToolUseStatus {
    pub fn from_envelope_kind(kind: ToolResultKind) -> Self {
        match kind {
            ToolResultKind::Ok => Self::Ok,
            ToolResultKind::Warning => Self::Warning,
            ToolResultKind::Error => Self::Error,
            ToolResultKind::Cancelled => Self::Cancelled,
        }
    }

    /// Severity used when emitting an [`ShellActivityEntry`].
    /// Delegates to `ToolUseUiStatus` — single source of truth for
    /// Ok/Warn/Error/Cancelled/Pending mapping across D9 + D10.
    pub fn severity(self) -> Severity {
        let ui = match self {
            Self::Ok => ToolUseUiStatus::Ok,
            Self::Warning => ToolUseUiStatus::Warning,
            Self::Error => ToolUseUiStatus::Error,
            Self::Cancelled => ToolUseUiStatus::Cancelled,
            Self::Pending => ToolUseUiStatus::Pending,
        };
        ui.severity()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
            Self::Pending => "pending",
        }
    }
}

/// One operator-safe projection of a transcript tool-use row.
/// `summary`, `duration_ms`, and `transcript_path` are the
/// only fields rendered into [`ShellActivityEntry::detail`]
/// today; the raw envelope payload + arguments stay on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseActivityProjection {
    pub call_id: String,
    pub tool_name: String,
    pub status: ToolUseStatus,
    pub summary: String,
    pub duration_ms: u64,
    pub transcript_path: PathBuf,
}

impl ToolUseActivityProjection {
    /// Build the operator-visible activity entry. Title and
    /// detail are intentionally short — no payload, no
    /// arguments, no secrets.
    pub fn to_activity_entry(&self, ts_unix: u64) -> ShellActivityEntry {
        let severity = self.status.severity();
        let kind = match self.status {
            ToolUseStatus::Ok
            | ToolUseStatus::Warning
            | ToolUseStatus::Cancelled
            | ToolUseStatus::Pending => ShellActivityKind::ToolResult,
            // Error rows still tag as ToolResult — the kind
            // names the source of the activity, the severity
            // names the outcome.
            ToolUseStatus::Error => ShellActivityKind::ToolResult,
        };
        let title = format!("{} {}", self.tool_name, self.status.as_str());
        let detail = Some(format!(
            "summary: {}\nduration_ms: {}\ntranscript: {}",
            self.summary,
            self.duration_ms,
            self.transcript_path.display()
        ));
        ShellActivityEntry {
            id: format!("toolproj-{}", self.call_id),
            ts_unix,
            kind,
            title,
            detail,
            severity,
        }
    }
}

/// Aggregate counts for the session-browser tile. Numbers add
/// up to `total_calls`; missing-result entries land in
/// `pending_count`, NOT in any of the kind-specific counters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolUseActivitySummary {
    pub transcript_path: PathBuf,
    pub total_calls: usize,
    pub ok_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub cancelled_count: usize,
    pub pending_count: usize,
}

/// Project every tool-use row in `transcript_path` into a
/// vector of operator-safe `ToolUseActivityProjection`s.
/// Missing transcript / pre-D7E transcript without tool rows
/// → `Ok(vec![])`.
pub fn project_tool_use_activity(
    transcript_path: impl AsRef<Path>,
) -> Result<Vec<ToolUseActivityProjection>, ProjectionError> {
    let path = transcript_path.as_ref();
    let views = read_tool_use_rows(path)?;
    Ok(views
        .into_iter()
        .map(|v| project_one(v, path.to_path_buf()))
        .collect())
}

/// Aggregate counts via [`project_tool_use_activity`]. Same
/// tolerance (missing file → zeroed summary).
pub fn summarize_tool_use(
    transcript_path: impl AsRef<Path>,
) -> Result<ToolUseActivitySummary, ProjectionError> {
    let path = transcript_path.as_ref();
    let projections = project_tool_use_activity(path)?;
    let mut summary = ToolUseActivitySummary {
        transcript_path: path.to_path_buf(),
        total_calls: projections.len(),
        ..Default::default()
    };
    for p in &projections {
        match p.status {
            ToolUseStatus::Ok => summary.ok_count += 1,
            ToolUseStatus::Warning => summary.warning_count += 1,
            ToolUseStatus::Error => summary.error_count += 1,
            ToolUseStatus::Cancelled => summary.cancelled_count += 1,
            ToolUseStatus::Pending => summary.pending_count += 1,
        }
    }
    Ok(summary)
}

/// Convenience for the session-browser tile.
/// Identical to [`summarize_tool_use`]; named separately so
/// future refactors can specialise the session-browser path
/// without breaking callers of the generic summariser.
pub fn session_tool_use_summary(
    transcript_path: impl AsRef<Path>,
) -> Result<ToolUseActivitySummary, ProjectionError> {
    summarize_tool_use(transcript_path)
}

fn project_one(view: ToolUseTranscriptView, transcript_path: PathBuf) -> ToolUseActivityProjection {
    let (status, summary, duration_ms) = match &view.result {
        Some(env) => (
            ToolUseStatus::from_envelope_kind(env.kind),
            env.summary.clone(),
            env.duration_ms,
        ),
        None => (
            ToolUseStatus::Pending,
            "(no result row recorded yet)".to_string(),
            0,
        ),
    };
    ToolUseActivityProjection {
        call_id: view.id,
        tool_name: view.name,
        status,
        summary,
        duration_ms,
        transcript_path,
    }
}
