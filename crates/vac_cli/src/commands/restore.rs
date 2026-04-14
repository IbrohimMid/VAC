//! `vac restore` — restore file to pre-agent state using snapshot journal.

use std::path::PathBuf;
use vac_core::session::Session;

pub async fn execute(project_root: PathBuf, file: PathBuf) -> anyhow::Result<()> {
    let session = Session::load_latest(&project_root)?
        .ok_or_else(|| anyhow::anyhow!("No active session found"))?;

    let file_str = if file.is_absolute() {
        file.to_string_lossy().to_string()
    } else {
        file.to_string_lossy().to_string()
    };

    vac_tools::journal::restore_snapshot(&project_root, session.id, &file_str)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("✓ Restored {}", file_str);
    Ok(())
}
