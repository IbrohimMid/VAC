//! Session-list DTO used by the shortcuts popup's Sessions tab and
//! by future session pickers. The widget never sees a VAC `Session`
//! type; the host projects to `SessionEntry` via the `VacPaths`
//! adapter and any disk-scanning helper that lives host-side.

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
