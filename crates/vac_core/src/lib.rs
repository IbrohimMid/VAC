//! VAC Core Engine

pub mod auth;
pub mod config;
pub mod detector;
pub mod engine;
pub mod error;
pub mod prelude;
pub mod profile;
pub mod session;
pub mod spawn_subtask_tool;
pub mod task;

pub use auth::{AuthStatus, StoredAuth};
pub use config::VacConfig;
pub use detector::{VilArchetype, VilProjectProfile};
pub use engine::{EngineStatus, RuntimeUpdate, TaskHistoryEntry, VacEngine};
pub use error::VacError;
pub use profile::{ProfileName, ProfileOverride};
pub use session::Session;
pub use spawn_subtask_tool::SpawnSubtaskTool;
pub use task::{Priority, Task, TaskConstraints, TaskResult, TaskStatus};
