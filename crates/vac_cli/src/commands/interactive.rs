//! `vac interactive` — full-screen TUI mode.

use std::path::PathBuf;

pub async fn execute(
    project_root: PathBuf,
    resume: bool,
    record: Option<PathBuf>,
    replay: Option<PathBuf>,
) -> anyhow::Result<()> {
    let io_mode = vac_tui_runtime::TuiIoMode {
        record_dir: record,
        replay_file: replay,
    };
    vac_tui_runtime::run_vac_tui_with_io(project_root, resume, io_mode).await
}
