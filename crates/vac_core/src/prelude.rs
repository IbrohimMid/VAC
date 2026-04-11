//! Prelude module — re-exports commonly used types.

pub use crate::config::VacConfig;
pub use crate::engine::VacEngine;
pub use crate::error::{VacError, VacResult};
pub use crate::session::Session;
pub use crate::task::{Priority, Task, TaskId, TaskResult, TaskStatus};
