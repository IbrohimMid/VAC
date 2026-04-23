//! `SessionMetaState` — metadata about the active session that isn't
//! operator cursor state and isn't runtime job state. Keeps the
//! `session_*` flat fields from scattering across AppState.

use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct SessionMetaState {
    /// Human-readable title shown in the header (derived from the first
    /// user prompt or set explicitly via /title).
    pub title: Option<String>,
    /// Path to the last written checkpoint, if any. Used by `vac resume`.
    pub checkpoint_path: Option<PathBuf>,
    /// True from boot until the deferred session snapshot load
    /// completes; drives the footer "restoring session..." placeholder.
    pub loading: bool,
}
