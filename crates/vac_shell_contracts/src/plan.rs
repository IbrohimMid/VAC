//! Slice 20.1 — plan DTOs lifted from `vac_shell_plan` so the plan
//! view widget can stay on the canonical `ratatui +
//! vac_shell_contracts` UI dep graph. Parser logic stays in
//! `vac_shell_plan`; this module is data-only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    #[default]
    Draft,
    Active,
    Blocked,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlanMetadata {
    pub status: PlanStatus,
    #[serde(default)]
    pub objective: Option<String>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub blocked_on: Vec<String>,
}
