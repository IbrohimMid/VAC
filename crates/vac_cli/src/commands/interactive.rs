//! `vac interactive` — full-screen TUI mode.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, _resume: bool) -> anyhow::Result<()> {
    crate::tui::run(project_root, _resume).await
}
