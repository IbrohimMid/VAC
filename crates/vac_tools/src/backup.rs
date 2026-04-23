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

/// Length of the content hash prefix. 16 hex chars = 64 bits. A
/// project memdir of thousands of files has a negligible birthday-
/// collision probability; the full id also contains a path-hash
/// segment so two distinct files that happen to share the same
/// content prefix never overwrite each other's metadata.
const HASH_PREFIX_LEN: usize = 16;
/// Path-hash prefix used to disambiguate snapshots of different files
/// with identical content. 8 hex chars = 32 bits — enough to separate
/// every file in a project tree.
const PATH_HASH_PREFIX_LEN: usize = 8;

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
    /// The submit_id during which this backup was taken.
    pub submit_id: Option<uuid::Uuid>,
}

fn hash_prefix(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    hex[..HASH_PREFIX_LEN].to_string()
}

fn path_prefix(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_os_str().as_encoded_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    hex[..PATH_HASH_PREFIX_LEN].to_string()
}

/// Compose the full backup id: `<content_hash>_<path_hash>`. Same
/// content under different paths gets different ids so dedup is
/// content-addressed WITHIN a path and restore always points at the
/// caller's original location.
fn compose_id(content: &str, path_hash: &str) -> String {
    format!("{content}_{path_hash}")
}

/// Return a canonical, absolute form of `path`. Falls back to
/// `project_root.join(path)` when the path does not yet exist (e.g.
/// snapshotting a create-new-file).
fn canonicalize_or_join(project_root: &Path, path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_root.join(path)
            }
        }
    }
}

/// Build a unique tmp sibling path for an atomic rename. Appends the
/// suffix to the existing filename (never replaces an extension) so
/// multi-dot names like `<id>.meta.json` stay distinct from their
/// tmps.
fn tmp_sibling(final_path: &Path, pid: u32, nonce: i64) -> PathBuf {
    let mut name = final_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{pid}.{nonce}.tmp"));
    final_path.with_file_name(name)
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
    submit_id: Option<uuid::Uuid>,
) -> Result<BackupRecord, ToolError> {
    let dir = backups_dir(project_root);
    fs::create_dir_all(&dir)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    let canonical = canonicalize_or_join(project_root, path);
    let bytes = match fs::read(&canonical).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
    };
    let content_hash = hash_prefix(&bytes);
    let path_hash = path_prefix(&canonical);
    let id = compose_id(&content_hash, &path_hash);
    let snap = snap_path(project_root, &id);
    let meta = meta_path(project_root, &id);

    // Only write if not already present (content+path-addressed dedup).
    let already = fs::try_exists(&snap).await.unwrap_or(false)
        && fs::try_exists(&meta).await.unwrap_or(false);
    if !already {
        let pid = std::process::id();
        let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let snap_tmp = tmp_sibling(&snap, pid, nonce);
        let meta_tmp = tmp_sibling(&meta, pid, nonce);
        fs::write(&snap_tmp, &bytes)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let record = BackupRecord {
            id: id.clone(),
            original_path: canonical.clone(),
            taken_at: chrono::Utc::now(),
            size_bytes: bytes.len() as u64,
            submit_id,
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
    // originally-recorded path/timestamp. Because `id` includes the
    // path hash, this entry is guaranteed to correspond to the same
    // caller path.
    let raw = fs::read(&meta)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
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

/// M2.2 — Return all backups matching the given submit_id
pub async fn list_for_submit(project_root: &Path, submit_id: uuid::Uuid) -> Result<Vec<BackupRecord>, ToolError> {
    let all = list_backups(project_root).await?;
    let mut out: Vec<_> = all.into_iter().filter(|r| r.submit_id == Some(submit_id)).collect();
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
        let rec = snapshot_file(root, &file, None).await.unwrap();
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
        let rec = snapshot_file(root, &ghost, None).await.unwrap();
        assert_eq!(rec.size_bytes, 0);
        // Simulate a create-new edit.
        fs::write(&ghost, b"created by tool").await.unwrap();
        restore_backup(root, &rec.id).await.unwrap();
        assert!(!fs::try_exists(&ghost).await.unwrap());
    }

    #[tokio::test]
    async fn same_content_different_paths_get_distinct_ids() {
        // Prior behavior: identical content collapsed to the same
        // snapshot and the second caller got back the FIRST path.
        // Fixed: id = content_hash + path_hash, so each caller's
        // record points at their own path and restore is never
        // cross-wired.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        fs::write(&a, b"same").await.unwrap();
        fs::write(&b, b"same").await.unwrap();
        let r1 = snapshot_file(root, &a, None).await.unwrap();
        let r2 = snapshot_file(root, &b, None).await.unwrap();
        assert_ne!(r1.id, r2.id, "distinct paths must get distinct ids");
        assert!(r1.original_path.ends_with("a.txt"));
        assert!(r2.original_path.ends_with("b.txt"));
    }

    #[tokio::test]
    async fn same_path_same_content_dedups_to_one_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a = root.join("a.txt");
        fs::write(&a, b"hello").await.unwrap();
        let r1 = snapshot_file(root, &a, None).await.unwrap();
        let r2 = snapshot_file(root, &a, None).await.unwrap();
        assert_eq!(r1.id, r2.id);
        assert_eq!(r1.taken_at, r2.taken_at, "dedup preserves first timestamp");
    }

    #[tokio::test]
    async fn relative_path_is_canonicalized_into_record() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a = root.join("rel.txt");
        fs::write(&a, b"x").await.unwrap();
        let rec = snapshot_file(root, std::path::Path::new("rel.txt"), None)
            .await
            .unwrap();
        // original_path is now absolute regardless of caller input.
        assert!(rec.original_path.is_absolute(), "{:?}", rec.original_path);
    }

    #[tokio::test]
    async fn list_backups_orders_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let f1 = root.join("one.txt");
        let f2 = root.join("two.txt");
        fs::write(&f1, b"first").await.unwrap();
        snapshot_file(root, &f1, None).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        fs::write(&f2, b"second").await.unwrap();
        snapshot_file(root, &f2, None).await.unwrap();
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
