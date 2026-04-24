//! VAC CLI library — exposes TUI modules + W8 commands for testing.
//!
//! The full `commands` tree pulls in types declared in main.rs
//! (`AuthAction`, `ConfigAction`, `SignalCommand`) that can't be
//! reached from the library root. Instead we re-export each W8
//! command module directly — the library stays small, the
//! integration smoke tests stay honest (same code path as the bin).
#[path = "commands/diagnostics.rs"]
pub mod diagnostics;
#[path = "commands/integrations.rs"]
pub mod integrations;
#[path = "commands/onboard.rs"]
pub mod onboard;
#[path = "commands/plan_memory.rs"]
pub mod plan_memory;
#[path = "commands/review.rs"]
pub mod review;

pub mod io;

/// Namespace expected by the W8 smoke tests.
pub mod commands {
    pub use super::diagnostics;
    pub use super::integrations;
    pub use super::onboard;
    pub use super::plan_memory;
    pub use super::review;
}
