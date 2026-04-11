//! Agent definitions and lifecycle.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;
use vil_trust::TrustZone;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    Architect,
    Coder,
    Tester,
    Security,
    Deploy,
    Monitor,
    Optimizer,
    Documenter,
}

impl AgentRole {
    pub fn system_prompt(&self) -> &str {
        match self {
            Self::Architect => {
                "You are a system architect. Decompose tasks into subtasks, design module boundaries, define API contracts, and plan implementation order."
            }
            Self::Coder => {
                "You are a Rust developer following the VIL Way. Write clean, type-safe, well-documented Rust code with proper error handling, borrow semantics, and tests."
            }
            Self::Tester => {
                "You are a test engineer. Generate comprehensive unit tests, integration tests, mutation tests, and ensure high code coverage."
            }
            Self::Security => {
                "You are a security auditor. Scan for vulnerabilities, check dependency safety, validate input handling, and enforce OWASP best practices."
            }
            Self::Deploy => {
                "You are a deployment engineer. Manage CI/CD pipelines, canary releases, rollbacks, and infrastructure configuration."
            }
            Self::Monitor => {
                "You are an observability engineer. Set up logging, metrics, alerting, anomaly detection, and SLA tracking."
            }
            Self::Optimizer => {
                "You are a performance engineer. Profile code, optimize hot paths, tune memory usage, reduce binary size, and improve SHM efficiency."
            }
            Self::Documenter => {
                "You are a documentation specialist. Write API docs, architecture diagrams, changelogs, and user guides."
            }
        }
    }

    pub fn default_trust_zone(&self) -> TrustZone {
        match self {
            Self::Architect | Self::Coder | Self::Tester | Self::Documenter => TrustZone::Trusted,
            Self::Security | Self::Monitor => TrustZone::Untrusted,
            Self::Deploy | Self::Optimizer => TrustZone::Trusted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecClass {
    Realtime,
    Background,
    Batch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: AgentId,
    pub name: String,
    pub role: AgentRole,
    pub trust_zone: TrustZone,
    pub exec_class: ExecClass,
    pub max_concurrent_tasks: usize,
    pub checkpoint_interval: Duration,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Working,
    Paused,
    Failed,
    Terminated,
}

impl AgentDefinition {
    pub fn new(role: AgentRole) -> Self {
        Self {
            id: AgentId::new(),
            name: format!("{:?}-agent", role).to_lowercase(),
            role,
            trust_zone: role.default_trust_zone(),
            exec_class: ExecClass::Background,
            max_concurrent_tasks: 1,
            checkpoint_interval: Duration::from_secs(30),
            status: AgentStatus::Idle,
        }
    }
}
