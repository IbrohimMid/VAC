//! VAC Core Engine

pub mod acp;
pub mod approval;
pub mod auth;
pub mod config;
pub mod detector;
pub mod engine;
pub mod error;
pub mod lsp;
pub mod prelude;
pub mod profile;
pub mod rulebook;
pub mod security;
pub mod session;
pub mod snapshot;
pub mod spawn_subtask_tool;
pub mod task;

pub use acp::AcpServer;
pub use approval::{
    ApprovalHandle, ApprovalRecord, ApprovalState, ApprovalStateMachine, ApprovalStore,
};
pub use auth::{AuthStatus, StoredAuth};
pub use config::{RulebookConfig, RuntimeConfig, VacConfig, VilLspConfig};
pub use detector::{VilArchetype, VilProjectProfile};
pub use engine::{EngineStatus, RuntimeUpdate, TaskHistoryEntry, VacEngine};
pub use error::VacError;
pub use profile::{ProfileName, ProfileOverride};
pub use security::{SecretDetector, SecretSubstitution};
pub use session::Session;
pub use spawn_subtask_tool::SpawnSubtaskTool;
pub use task::{Priority, Task, TaskConstraints, TaskResult, TaskStatus};
