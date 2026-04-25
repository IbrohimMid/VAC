//! Approval bridge — the donor shell renders pending approvals and
//! collects operator decisions, but the *decision policy* (auto-approve,
//! per-tool trust, hook gate, rulebook) lives in VAC. The donor
//! `AutoApproveManager` and friends are explicitly NOT used; the shell
//! bridge calls into `VacApprovalBridge` for both queue state and
//! resolution.
//!
//! Blocker B in the donor extraction map: this trait is the seam that
//! prevents two systems from racing to approve the same call.

#![allow(clippy::module_inception)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    /// One-line summary suitable for a list row (e.g. the path or
    /// command being acted on).
    pub summary: String,
    /// Risk classification driven by VAC policy (`READ`, `WRITES`,
    /// `DESTRUCTIVE`, `ELEVATED`). Donor renders without re-classifying.
    pub risk: String,
    /// Rulebook clause that triggered the prompt, if any.
    pub policy_clause: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approve,
    ApproveAndRemember,
    Reject { reason: Option<String> },
    Defer,
}

#[async_trait::async_trait]
pub trait VacApprovalBridge: Send + Sync {
    /// Snapshot of pending approvals in arrival order.
    async fn pending(&self) -> Vec<PendingApproval>;

    /// Resolve a single approval. Returns once VAC has accepted the
    /// decision; the actual tool dispatch is asynchronous.
    async fn resolve(&self, id: &str, decision: ApprovalDecision) -> Result<(), ApprovalError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("approval id not found: {0}")]
    NotFound(String),
    #[error("approval already resolved: {0}")]
    AlreadyResolved(String),
    #[error("bridge error: {0}")]
    Other(String),
}

/// Slice 16 — risk classification surfaced in the detail drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Slice 16 — full detail view for the currently-selected approval.
/// Compact bar still uses the existing `PendingApproval`; this is
/// what the drawer renders when the operator wants more context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDetailView {
    pub id: String,
    pub tool_name: String,
    pub risk_level: RiskLevel,
    pub reason: String,
    pub command_preview: Option<String>,
    pub file_preview: Option<String>,
    pub policy_source: Option<String>,
}
