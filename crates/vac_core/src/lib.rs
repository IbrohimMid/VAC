//! VAC Core Engine
//!
//! Central orchestrator that ties together all VIL subsystems:
//! IR pipeline, context engine, memory store, agent swarm, tools, and tracing.

pub mod config;
pub mod engine;
pub mod error;
pub mod prelude;
pub mod session;
pub mod task;

pub use config::VacConfig;
pub use engine::VacEngine;
pub use error::VacError;
pub use session::Session;
pub use task::{Priority, Task, TaskConstraints, TaskResult, TaskStatus};
