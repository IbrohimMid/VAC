//! `vac restore` — VIL-native state recovery from snapshot journal.
//!
//! Operates at session level: finds the most recent snapshot for the
//! given file within the active session, verifies integrity via hash,
//! then restores. Prints structured feedback for operator awareness.

use std::path::PathBuf;
use vac_core::session::Session;

pub async fn execute(project_root: PathBuf, file: PathBuf) -> anyhow::Result<()> {
    let session = Session::load_latest(&project_root)?
        .ok_or_else(|| anyhow::anyhow!("No active session. Run `vac init` first."))?;

    let file_str = file.to_string_lossy().to_string();

    // List snapshots for this session to give operator context
    let snapshots = vac_tools::journal::list_snapshots(&project_root, session.id);
    if snapshots.is_empty() {
        anyhow::bail!(
            "No snapshots found for session {}.\n\
             Snapshots are created automatically before agent file modifications.",
            &session.id.to_string()[..8]
        );
    }

    if !snapshots.contains(&file_str) {
        anyhow::bail!(
            "No snapshot for '{}' in session {}.\n\
             Available snapshots:\n{}",
            file_str,
            &session.id.to_string()[..8],
            snapshots.iter().map(|s| format!("  - {s}")).collect::<Vec<_>>().join("\n")
        );
    }

    vac_tools::journal::restore_snapshot(&project_root, session.id, &file_str)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!(
        "✓ Restored '{}' (session: {})",
        file_str,
        &session.id.to_string()[..8]
    );
    Ok(())
}
