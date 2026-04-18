//! `vac interactive` — full-screen TUI mode.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, resume: bool) -> anyhow::Result<()> {
    vac_tui_runtime::run_vac_tui(project_root, resume).await
}
