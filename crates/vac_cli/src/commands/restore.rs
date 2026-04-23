//! `vac restore` — VIL-native state recovery from snapshot journal.
//!
//! Operates at session level: finds the most recent snapshot for the
//! given file within the active session, verifies integrity via hash,
//! then restores. Prints structured feedback for operator awareness.

use std::path::PathBuf;
use vac_core::session::Session;

pub async fn execute(
    project_root: PathBuf,
    file: Option<PathBuf>,
    submit: Option<uuid::Uuid>,
) -> anyhow::Result<()> {
    if let Some(submit_id) = submit {
        let backups = vac_tools::backup::list_for_submit(&project_root, submit_id).await?;
        if backups.is_empty() {
            anyhow::bail!("No backups found for submit_id {}", submit_id);
        }
        for b in backups {
            vac_tools::backup::restore_backup(&project_root, &b.id).await?;
            println!("✓ Restored '{}' from submit {}", b.original_path.display(), submit_id);
        }
        return Ok(());
    }

    let file = file.unwrap();
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
            snapshots
                .iter()
                .map(|s| format!("  - {s}"))
                .collect::<Vec<_>>()
                .join("\n")
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
