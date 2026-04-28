//! D18 — init checklist DTOs for the first-run readiness surface.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum InitChecklistStatus {
    #[default]
    Unknown,
    Ready,
    Warning,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum InitChecklistAction {
    #[default]
    None,
    OpenDoctor,
    OpenStatus,
    OpenLogs,
    OpenSessions,
    OpenModelSwitcher,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitChecklistRow {
    pub id: String,
    pub label: String,
    pub status: InitChecklistStatus,
    pub summary: String,
    pub detail: Option<String>,
    pub action: InitChecklistAction,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InitChecklistViewModel {
    pub title: String,
    pub rows: Vec<InitChecklistRow>,
    pub next_action: Option<String>,
}
