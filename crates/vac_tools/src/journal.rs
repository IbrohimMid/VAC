//! Reversible file operation journal.
//!
//! Before any file mutation (write/edit/delete), snapshot the original content.
//! Snapshots are stored in `.vac/backups/<session_id>/<task_id>/`.
//! Exposes: snapshot_before_write, list_snapshots, restore_snapshot.

use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Take a snapshot of a file before mutation.
/// Returns the snapshot path, or None if the file doesn't exist yet (new file).
pub fn snapshot_before_write(
    working_dir: &Path,
    session_id: Uuid,
    file_path: &str,
) -> Option<PathBuf> {
    let abs_path = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        working_dir.join(file_path)
    };

    if !abs_path.exists() {
        return None; // New file — nothing to snapshot
    }

    let backup_dir = working_dir
        .join(".vac/backups")
        .join(session_id.to_string());
    let _ = std::fs::create_dir_all(&backup_dir);

    // Use a sanitized filename: replace path separators with __ and add .bak
    let safe_name = format!("{}.bak", file_path.replace(['/', '\\'], "__"));
    let snapshot_path = backup_dir.join(&safe_name);

    if let Err(e) = std::fs::copy(&abs_path, &snapshot_path) {
        tracing::warn!(file = %file_path, error = %e, "Failed to snapshot file before write");
        return None;
    }

    tracing::debug!(file = %file_path, snapshot = %snapshot_path.display(), "Snapshotted file");
    Some(snapshot_path)
}

/// List all snapshots for a session.
pub fn list_snapshots(working_dir: &Path, session_id: Uuid) -> Vec<String> {
    let backup_dir = working_dir.join(".vac/backups").join(session_id.to_string());
    if !backup_dir.exists() {
        return vec![];
    }
    std::fs::read_dir(&backup_dir)
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .map(|name| {
                    name.trim_end_matches(".bak")
                        .replace("__", "/")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Restore a file from its snapshot.
pub fn restore_snapshot(
    working_dir: &Path,
    session_id: Uuid,
    file_path: &str,
) -> Result<(), String> {
    let backup_dir = working_dir.join(".vac/backups").join(session_id.to_string());
    let safe_name = format!("{}.bak", file_path.replace(['/', '\\'], "__"));
    let snapshot_path = backup_dir.join(&safe_name);

    if !snapshot_path.exists() {
        return Err(format!("No snapshot found for '{file_path}' in session {session_id}"));
    }

    let abs_path = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        working_dir.join(file_path)
    };

    if let Some(parent) = abs_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::copy(&snapshot_path, &abs_path)
        .map(|_| ())
        .map_err(|e| format!("Failed to restore '{file_path}': {e}"))
}
