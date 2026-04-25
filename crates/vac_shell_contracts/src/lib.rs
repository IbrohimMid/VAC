//! VAC ↔ Stakpak shell adapter contracts.
//!
//! This crate is a deliberately tiny surface. It declares the trait
//! and DTO boundaries that the VAC core exposes to a shell donor
//! (currently Stakpak) so the shell never reaches into VAC types
//! directly and vice versa.
//!
//! # Boundary discipline
//!
//! - This crate has **no implementations** of its traits. Implementations
//!   live in `vac_shell_bridge` (to be added in Step 2 of the donor
//!   extraction plan) which depends on both this crate and the VAC
//!   semantic crates (`vac_core`, `vac_session_engine`, …).
//! - Donor components consume these traits through trait objects so
//!   they remain decoupled from VAC internals.
//! - When in doubt, prefer **narrow** types here over re-exporting VAC
//!   types — drift across this boundary is the failure mode this crate
//!   exists to prevent.
//!
//! See `docs/DONOR_EXTRACTION_MAP.md` for the rationale and the
//! component-by-component reusability decisions that drive this
//! surface.

pub mod paths;
pub mod model;
pub mod approval;
pub mod registry;
pub mod events;
pub mod session;
pub mod selection;

pub use approval::VacApprovalBridge;
pub use events::{VacShellEvent, VacSubmitRequest};
pub use model::{ProviderId, VacModelView};
pub use paths::VacPaths;
pub use registry::{ShellCommandKind, ShellCommandSpec, VacCommandRegistry};
pub use selection::{ModelKey, ModelSelectionSnapshot};
pub use session::SessionEntry;
