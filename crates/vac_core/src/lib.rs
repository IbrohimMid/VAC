//! VAC Core Engine

pub mod acp;
pub mod auth;
pub mod bundle;
pub mod config;
pub mod detector;
pub mod engine;
pub mod error;
pub mod lsp;
pub mod policy_gate;
pub mod prelude;
pub mod profile;
pub mod rulebook;
pub mod security;
pub mod session;
pub mod snapshot;
pub mod spawn_subtask_tool;
pub mod task;

pub use acp::AcpServer;
pub use auth::{AuthStatus, StoredAuth};
pub use bundle::{BundleMetadata, VacBundle};
pub use config::{
    ExecutionEnvironment, LlmConfig, LlmProviderConfig, NetworkPolicy, RulebookConfig,
    RuntimeConfig, VacConfig, VilConfig, VilLspConfig,
};
pub use detector::{VilArchetype, VilProjectProfile};
pub use engine::{EngineStatus, RuntimeUpdate, TaskHistoryEntry, VacEngine};
pub use error::VacError;
pub use policy_gate::{PolicyGateAction, PolicyGateDecision, PolicyGateMode};
pub use profile::{ProfileName, ProfileOverride};
pub use security::{SecretDetector, SecretSubstitution};
pub use session::Session;
pub use spawn_subtask_tool::SpawnSubtaskTool;
pub use task::{Priority, Task, TaskConstraints, TaskResult, TaskStatus};
pub use vac_approvals::{
    ActiveApprovalRegistry, ApprovalHandle, ApprovalRecord, ApprovalState, ApprovalStore,
};
