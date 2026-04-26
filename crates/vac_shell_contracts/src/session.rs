//! Session-list DTO used by the shortcuts popup's Sessions tab and
//! by future session pickers. The widget never sees a VAC `Session`
//! type; the host projects to `SessionEntry` via the `VacPaths`
//! adapter and any disk-scanning helper that lives host-side.

use crate::ToolUseUiStatus;
use serde::{Deserialize, Serialize};

/// One row in a sessions list. Identifier is opaque — the host maps
/// it back to whatever VAC session record owns the actual transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEntry {
    /// Session identifier (typically a UUID string).
    pub id: String,
    /// Display label — operator-facing summary or the session title.
    pub label: String,
    /// Last-modified timestamp in seconds since the Unix epoch.
    /// `0` is acceptable when the host does not have one.
    pub last_active_unix: u64,
}

/// Slice 13 — read-only transcript preview the host hands the UI.
/// First line is typically the user's opening prompt; following
/// lines are an excerpt of the agent reply. Pure DTO, no semantic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionPreview {
    pub id: String,
    pub title: Option<String>,
    pub lines: Vec<String>,
}

/// Slice 13 — operator intent for a session row. UI emits these;
/// host applies them through its own controller (delete / archive
/// / resume / open-preview semantics live host-side).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionAction {
    Open { id: String },
    Resume { id: String },
    Archive { id: String },
    Delete { id: String },
}

/// D10 — aggregate tool-use counts for one session transcript.
/// Populated by the host (via `vac_shell_host_transcript_projection`)
/// when the session browser opens. `None` on the tile means the host
/// could not produce a summary (missing file, pre-D7E transcript).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionToolSummary {
    pub total_calls: usize,
    pub ok_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub cancelled_count: usize,
    pub pending_count: usize,
}

impl SessionToolSummary {
    pub fn badge_text(&self) -> String {
        if self.total_calls == 0 {
            "no tools".to_string()
        } else {
            format!("tools: {} ok / {} err", self.ok_count, self.error_count)
        }
    }
}

/// D11 — detailed record of a single tool call for the session browser UI.
/// Only contains operator-safe fields (status, name, summary, duration).
/// Explicitly excludes raw payload or arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionToolUseDetail {
    pub call_id: String,
    pub tool_name: String,
    pub status: ToolUseUiStatus,
    pub summary: String,
    pub duration_ms: u64,
}

/// D11 — wrapper struct containing both summary and detailed rows,
/// returned by the host provider callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionToolUseSurface {
    pub summary: SessionToolSummary,
    pub calls: Vec<SessionToolUseDetail>,
}

/// D10 — one tile in the session browser list. Wraps `SessionEntry`
/// with an optional tool-use summary badge and (in D11) a list of tool call details.
/// Widget renders the badge and detail preview; host populates them via a closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTileView {
    pub entry: SessionEntry,
    pub tool_summary: Option<SessionToolSummary>,
    #[serde(default)]
    pub tool_details: Vec<SessionToolUseDetail>,
}
