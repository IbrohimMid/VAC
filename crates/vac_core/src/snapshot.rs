//! Snapshot manifest for file backup/restore operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub task_id: String,
    pub session_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub files: Vec<SnapshotFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct RestoreReport {
    pub restored_count: usize,
    pub failed: Vec<(PathBuf, String)>,
}

impl SnapshotManifest {
    /// Create snapshot manifest for given files.
    pub fn create_snapshot(
        task_id: String,
        session_id: uuid::Uuid,
        files: Vec<(PathBuf, PathBuf)>, // (original, backup)
    ) -> Result<Self> {
        let snapshot_files = files
            .into_iter()
            .map(|(original, backup)| {
                let hash = if backup.exists() {
                    compute_file_hash(&backup).unwrap_or_default()
                } else {
                    String::new()
                };
                SnapshotFile {
                    original_path: original,
                    backup_path: backup,
                    hash,
                }
            })
            .collect();

        Ok(Self {
            task_id,
            session_id,
            created_at: chrono::Utc::now(),
            files: snapshot_files,
        })
    }

    /// Save manifest to disk.
    pub fn save(&self, manifest_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(manifest_dir)?;
        let path = manifest_dir.join(format!("{}.manifest.json", self.task_id));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load manifest from disk.
    pub fn load(manifest_path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(manifest_path).context("Failed to read manifest")?;
        let manifest: Self = serde_json::from_str(&json).context("Failed to parse manifest")?;
        Ok(manifest)
    }

    /// Restore files from this manifest.
    pub fn restore(&self) -> Result<RestoreReport> {
        let mut restored = 0;
        let mut failed = Vec::new();

        for file in &self.files {
            if !file.backup_path.exists() {
                failed.push((file.original_path.clone(), "Backup not found".to_string()));
                continue;
            }

            // Verify hash if available
            if !file.hash.is_empty() {
                let current_hash = compute_file_hash(&file.backup_path).unwrap_or_default();
                if current_hash != file.hash {
                    failed.push((file.original_path.clone(), "Hash mismatch".to_string()));
                    continue;
                }
            }

            // Restore file
            if let Err(e) = std::fs::copy(&file.backup_path, &file.original_path) {
                failed.push((file.original_path.clone(), e.to_string()));
            } else {
                restored += 1;
            }
        }

        Ok(RestoreReport {
            restored_count: restored,
            failed,
        })
    }
}

/// List all snapshot manifests in directory.
pub fn list_snapshots(manifest_dir: &Path) -> Result<Vec<SnapshotManifest>> {
    if !manifest_dir.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(manifest_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(manifest) = SnapshotManifest::load(&path) {
                manifests.push(manifest);
            }
        }
    }

    // Sort by created_at descending
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(manifests)
}

fn compute_file_hash(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_restore_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original.txt");
        let backup = temp.path().join("backup.txt");

        std::fs::write(&original, "original content").unwrap();
        std::fs::write(&backup, "original content").unwrap();

        let manifest = SnapshotManifest::create_snapshot(
            "test-task".to_string(),
            uuid::Uuid::new_v4(),
            vec![(original.clone(), backup.clone())],
        )
        .unwrap();

        // Modify original
        std::fs::write(&original, "modified").unwrap();

        // Restore
        let report = manifest.restore().unwrap();
        assert_eq!(report.restored_count, 1);
        assert_eq!(
            std::fs::read_to_string(&original).unwrap(),
            "original content"
        );
    }

    #[test]
    fn save_and_load_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_dir = temp.path().join("manifests");

        let manifest = SnapshotManifest::create_snapshot(
            "test-task".to_string(),
            uuid::Uuid::new_v4(),
            vec![],
        )
        .unwrap();

        let path = manifest.save(&manifest_dir).unwrap();
        let loaded = SnapshotManifest::load(&path).unwrap();

        assert_eq!(loaded.task_id, "test-task");
    }
}
