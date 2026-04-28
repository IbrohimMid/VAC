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

pub mod activity;
pub mod approval;
pub mod diff;
pub mod events;
pub mod init;
pub mod model;
pub mod overlay;
pub mod paths;
pub mod plan;
pub mod redaction;
pub mod registry;
pub mod selection;
pub mod session;
pub mod shell;
pub mod status;
pub mod tool_ui_status;

pub use activity::{Severity, ShellActivityEntry, ShellActivityFilter, ShellActivityKind};
pub use approval::{ApprovalDetailView, RiskLevel, VacApprovalBridge};
pub use diff::{DiffFileView, DiffHunkView, DiffLineKind, DiffLineView, DiffReviewEvent};
pub use events::{VacShellEvent, VacSubmitRequest};
pub use init::{
    InitChecklistAction, InitChecklistRow, InitChecklistStatus, InitChecklistViewModel,
};
pub use model::{ProviderId, VacModelView};
pub use overlay::{OverlayIntent, ShellOverlay};
pub use paths::VacPaths;
pub use plan::{PlanMetadata, PlanStatus};
pub use redaction::{RedactionConfig, redact_json_value, redacted_json_preview};
pub use registry::{ShellCommandKind, ShellCommandSpec, VacCommandRegistry};
pub use selection::{ModelKey, ModelSelectionSnapshot};
pub use session::{
    SessionAction, SessionEntry, SessionPreview, SessionRecoveryStatus, SessionRecoverySummary,
    SessionTileView, SessionToolSummary, SessionToolUseDetail, SessionToolUseSurface,
};
pub use shell::{ShellCommandView, ShellStatus};
pub use status::ShellStatusView;
pub use tool_ui_status::ToolUseUiStatus;
