//! Path adapter — maps every donor `.stakpak/...` path lookup to a
//! VAC-canonical `.vac/...` location. The donor extraction map flagged
//! `.stakpak/` hardcoding as Blocker A; this trait is the single seam
//! through which the shell bridge satisfies all of those reads.
//!
//! Implementations must be deterministic and side-effect free —
//! callers may invoke them on the render path.

use std::path::PathBuf;

/// Logical path categories the donor shell asks for. Concrete on-disk
/// layout is owned by VAC; the donor never composes paths itself.
pub trait VacPaths: Send + Sync {
    /// Project root (e.g. `~/code/foo`). Equivalent to donor `cwd`.
    fn project_root(&self) -> PathBuf;

    /// Per-project VAC config dir (`<project>/.vac`). Donors that look
    /// for `.stakpak` should be redirected here.
    fn project_state_dir(&self) -> PathBuf;

    /// User-global VAC config dir (e.g. `~/.config/vac`).
    fn user_state_dir(&self) -> PathBuf;

    /// Directory holding session transcripts (`<state>/sessions`).
    fn sessions_dir(&self) -> PathBuf;

    /// Directory holding the active plan markdown if any
    /// (`<state>/session/plan.md` in donor terms).
    fn plan_file(&self) -> PathBuf;

    /// Directory holding user-defined custom slash commands
    /// (`<state>/commands`).
    fn commands_dir(&self) -> PathBuf;
}
