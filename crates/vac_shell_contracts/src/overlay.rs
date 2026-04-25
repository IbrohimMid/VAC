//! Slice 11 — overlay model.
//!
//! `ShellOverlay` enumerates the reachable popup surfaces.
//! `OverlayIntent` is the host-facing operation the
//! `OverlayStack` host crate applies. Both stay DTO-only — the
//! stack itself lives host-side in `vac_shell_overlay`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShellOverlay {
    None,
    Palette,
    Shortcuts,
    ModelSwitcher,
    ShellPopup,
    Plan,
    DiffReview,
    SessionBrowser,
    /// Slice 20.1 — approval detail drawer on top of the compact bar.
    ApprovalDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayIntent {
    Open(ShellOverlay),
    CloseTop,
    CloseAll,
    Toggle(ShellOverlay),
}
