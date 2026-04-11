//! Trust zones for agent isolation.

use serde::{Deserialize, Serialize};

/// Trust zones determine what resources an agent can access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrustZone {
    Trusted,
    Untrusted,
    Isolated,
}

impl TrustZone {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    FileRead,
    FileWrite,
    ShellExec,
    NetworkAccess,
    CodeAnalysis,
    Memory,
    GitOps,
    PackageManager,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    NeedsApproval,
    Dangerous,
}
