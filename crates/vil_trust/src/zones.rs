//! Trust zones for agent isolation.
//!
//! A [`TrustZone`] determines what categories of resource an agent can reach.
//! Zones are orthogonal to fine-grained [`crate::Permission`]s — zones are the
//! coarse gate, permissions the fine-tuning within a zone.

use serde::{Deserialize, Serialize};

/// Trust zones determine what resources an agent can access.
///
/// Zones form a strict containment hierarchy:
/// * `Trusted` — full access to all resource types and risk levels.
/// * `Untrusted` — read/analysis only; may request approval for risky actions.
/// * `Isolated` — sandboxed analysis only; cannot touch the filesystem or network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrustZone {
    /// Full access. Typically used for the local interactive agent.
    Trusted,
    /// Read-mostly access; writes and shell/network need explicit approval.
    Untrusted,
    /// Sandboxed: code analysis + memory only, no filesystem or network.
    Isolated,
}

impl TrustZone {
    /// Returns `true` if an agent in this zone may access the given resource.
    pub fn can_access(&self, resource: &ResourceType) -> bool {
        match (self, resource) {
            (TrustZone::Trusted, _) => true,
            (TrustZone::Untrusted, ResourceType::FileRead) => true,
            (TrustZone::Untrusted, ResourceType::CodeAnalysis) => true,
            (TrustZone::Untrusted, ResourceType::Memory) => true,
            (TrustZone::Untrusted, _) => false,
            (TrustZone::Isolated, ResourceType::CodeAnalysis) => true,
            (TrustZone::Isolated, ResourceType::Memory) => true,
            (TrustZone::Isolated, _) => false,
        }
    }

    /// Returns `true` if an agent in this zone may perform an action of the
    /// given risk level without further gating.
    #[allow(clippy::match_like_matches_macro)]
    pub fn allows_risk(&self, risk: &RiskLevel) -> bool {
        match (self, risk) {
            (_, RiskLevel::Safe) => true,
            (TrustZone::Trusted, _) => true,
            (TrustZone::Untrusted, RiskLevel::NeedsApproval) => true,
            _ => false,
        }
    }
}

/// Categories of resource an agent might try to access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    /// Reading files from the project (or filesystem more broadly).
    FileRead,
    /// Writing or modifying files.
    FileWrite,
    /// Executing shell commands.
    ShellExec,
    /// Making outbound network requests.
    NetworkAccess,
    /// Read-only code parsing / static analysis.
    CodeAnalysis,
    /// Accessing the agent's in-process memory store.
    Memory,
    /// Git repository operations (commit, push, branch).
    GitOps,
    /// Package manager operations (cargo add, npm install).
    PackageManager,
}

/// How risky an action is, independent of who is performing it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// No side effects; reversible; low cost.
    Safe,
    /// Has side effects but is bounded; requires human sign-off by default.
    NeedsApproval,
    /// Potentially destructive or hard to reverse; always gated.
    Dangerous,
}
