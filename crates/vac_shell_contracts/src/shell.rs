//! Slice 17 — shell-popup-v2 metadata DTOs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellCommandView {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub status: ShellStatus,
    pub exit_code: Option<i32>,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}
