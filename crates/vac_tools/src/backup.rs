//! F6.4 — Content-addressable file backup.
//!
//! Before a reversible edit lands (FileEditTool, FileWriteTool, any
//! future patcher), drivers call [`snapshot_file`] to archive the
//! current bytes under `.vac/backups/<sha256-prefix>.snap`. A
//! companion `.meta.json` carries the original path + timestamp so
//! `vac restore --backup <hash>` can put the bytes back where they
//! belong without relying on session state.
//!
//! Difference from [`crate::journal`]: the journal is session-scoped
//! and keyed by path; this module is session-independent and keyed
//! by content hash, which means identical contents dedup to the same
//! snapshot and a rollback still works after the session closes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;

use crate::error::ToolError;

/// Length of the hash prefix used as the snapshot id. 16 hex chars
/// = 64 bits of namespace — collision-resistant for a project-local
/// tree while keeping filenames short.
const HASH_PREFIX_LEN: usize = 16;

/// One entry in the backup tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRecord {
    /// Short sha256 prefix; also the filename stem on disk.
    pub id: String,
    /// Absolute path the snapshot was taken of.
    pub original_path: PathBuf,
    /// When the snapshot was taken.
    pub taken_at: chrono::DateTime<chrono::Utc>,
    /// Size of the snapshotted bytes.
    pub size_bytes: u64,
}

fn hash_prefix(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    hex[..HASH_PREFIX_LEN].to_string()
}

fn backups_dir(project_root: &Path) -> PathBuf {
    project_root.join(".vac").join("backups")
}

fn snap_path(project_root: &Path, id: &str) -> PathBuf {
    backups_dir(project_root).join(format!("{id}.snap"))
}

fn meta_path(project_root: &Path, id: &str) -> PathBuf {
    backups_dir(project_root).join(format!("{id}.meta.json"))
}

/// Snapshot the current contents of `path` into the backup tree and
/// return the record. Idempotent on content: if the hash already
/// exists the existing record is returned with `taken_at` untouched.
///
/// If `path` does not exist (e.g. the edit is a create-new-file), a
/// zero-byte snapshot is recorded so the restore can delete the
/// created file by truncating to the original empty state.
pub async fn snapshot_file(
    project_root: &Path,
    path: &Path,
) -> Result<BackupRecord, ToolError> {
    let dir = backups_dir(project_root);
    fs::create_dir_all(&dir).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    let bytes = match fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
    };
    let id = hash_prefix(&bytes);
    let snap = snap_path(project_root, &id);
    let meta = meta_path(project_root, &id);

    // Only write if not already present (content-addressed dedup).
    let already = fs::try_exists(&snap).await.unwrap_or(false)
        && fs::try_exists(&meta).await.unwrap_or(false);
    if !already {
        // Unique staging suffix — concurrent writers on the same
        // hash never collide on the tmp name.
        let pid = std::process::id();
        let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let snap_tmp = snap.with_extension(format!("snap.{pid}.{nonce}.tmp"));
        let meta_tmp = meta.with_extension(format!("json.{pid}.{nonce}.tmp"));
        fs::write(&snap_tmp, &bytes)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let record = BackupRecord {
            id: id.clone(),
            original_path: path.to_path_buf(),
            taken_at: chrono::Utc::now(),
            size_bytes: bytes.len() as u64,
        };
        let meta_bytes = serde_json::to_vec_pretty(&record)
            .map_err(|e| ToolError::ExecutionFailed(format!("meta serialize: {e}")))?;
        fs::write(&meta_tmp, &meta_bytes)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        fs::rename(&snap_tmp, &snap)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        fs::rename(&meta_tmp, &meta)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        return Ok(record);
    }

    // Existing entry — read its metadata back so callers see the
    // originally-recorded path/timestamp.
    let raw = fs::read(&meta).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    let record: BackupRecord = serde_json::from_slice(&raw)
        .map_err(|e| ToolError::ExecutionFailed(format!("meta parse: {e}")))?;
    Ok(record)
}

/// List every backup record under `<project>/.vac/backups/`, ordered
/// newest-first by `taken_at`.
pub async fn list_backups(project_root: &Path) -> Result<Vec<BackupRecord>, ToolError> {
    let dir = backups_dir(project_root);
    let mut entries = match fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
    {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_none_or(|n| !n.ends_with(".meta.json"))
        {
            continue;
        }
        let raw = fs::read(&path).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let record: BackupRecord = match serde_json::from_slice(&raw) {
            Ok(r) => r,
            Err(_) => continue, // skip malformed
        };
        out.push(record);
    }
    out.sort_by(|a, b| b.taken_at.cmp(&a.taken_at));
    Ok(out)
}

/// Restore the bytes recorded under `id` to their original path.
/// Returns the record that was restored. If the snapshot records an
/// empty file and the current path exists, the current file is
/// removed (reversing a create-new).
pub async fn restore_backup(
    project_root: &Path,
    id: &str,
) -> Result<BackupRecord, ToolError> {
    let meta = meta_path(project_root, id);
    let raw = fs::read(&meta)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("backup '{id}' not found: {e}")))?;
    let record: BackupRecord = serde_json::from_slice(&raw)
        .map_err(|e| ToolError::ExecutionFailed(format!("meta parse: {e}")))?;
    let snap = snap_path(project_root, id);
    let bytes = fs::read(&snap).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    if bytes.is_empty() {
        // Empty snapshot = file did not exist when snapshotted;
        // restore means "remove the post-edit file".
        if fs::try_exists(&record.original_path).await.unwrap_or(false) {
            fs::remove_file(&record.original_path)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }
        return Ok(record);
    }
    if let Some(parent) = record.original_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    }
    let pid = std::process::id();
    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let tmp = record
        .original_path
        .with_extension(format!("vac-restore.{pid}.{nonce}.tmp"));
    fs::write(&tmp, &bytes).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    fs::rename(&tmp, &record.original_path)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_then_restore_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let file = root.join("work.txt");
        fs::write(&file, b"original contents").await.unwrap();
        let rec = snapshot_file(root, &file).await.unwrap();
        fs::write(&file, b"mutated by tool").await.unwrap();
        restore_backup(root, &rec.id).await.unwrap();
        let back = fs::read(&file).await.unwrap();
        assert_eq!(back, b"original contents");
    }

    #[tokio::test]
    async fn snapshot_of_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let ghost = root.join("ghost.txt");
        let rec = snapshot_file(root, &ghost).await.unwrap();
        assert_eq!(rec.size_bytes, 0);
        // Simulate a create-new edit.
        fs::write(&ghost, b"created by tool").await.unwrap();
        restore_backup(root, &rec.id).await.unwrap();
        assert!(!fs::try_exists(&ghost).await.unwrap());
    }

    #[tokio::test]
    async fn identical_contents_dedup_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        fs::write(&a, b"same").await.unwrap();
        fs::write(&b, b"same").await.unwrap();
        let r1 = snapshot_file(root, &a).await.unwrap();
        let r2 = snapshot_file(root, &b).await.unwrap();
        // Same hash; metadata of the first entry wins.
        assert_eq!(r1.id, r2.id);
    }

    #[tokio::test]
    async fn list_backups_orders_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let f1 = root.join("one.txt");
        let f2 = root.join("two.txt");
        fs::write(&f1, b"first").await.unwrap();
        snapshot_file(root, &f1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        fs::write(&f2, b"second").await.unwrap();
        snapshot_file(root, &f2).await.unwrap();
        let all = list_backups(root).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].taken_at >= all[1].taken_at);
    }

    #[tokio::test]
    async fn restore_unknown_id_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let r = restore_backup(tmp.path(), "deadbeefcafe0000").await;
        assert!(r.is_err());
    }
}
