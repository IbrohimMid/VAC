//! Session control: snapshot, save/load, cleanup, and migration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

/// Current schema version for session snapshots.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Errors from session control operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionControlError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("session not found: {0}")]
    NotFound(Uuid),
    #[error("schema version {found} exceeds supported version {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("session is stale: last updated {0}")]
    Stale(DateTime<Utc>),
}

pub type Result<T> = std::result::Result<T, SessionControlError>;

/// Lightweight session snapshot for quick save/restore without full session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub project_root: PathBuf,
    pub active_model: Option<String>,
    pub active_profile: Option<String>,
    pub active_rulebooks: Vec<String>,
    pub task_count: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub total_tokens: u64,
    pub modified_files: usize,
    pub tui_state: TuiStateSnapshot,
    pub metadata: HashMap<String, String>,
}

/// TUI state that should survive session save/restore.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TuiStateSnapshot {
    pub active_tab_idx: Option<usize>,
    pub history_selection: Option<usize>,
    pub last_focus: Option<String>,
    pub collapsed_sections: Vec<String>,
}

impl SessionSnapshot {
    pub fn new(session_id: Uuid, project_root: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id,
            created_at: now,
            updated_at: now,
            project_root,
            active_model: None,
            active_profile: None,
            active_rulebooks: Vec::new(),
            task_count: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            total_tokens: 0,
            modified_files: 0,
            tui_state: TuiStateSnapshot::default(),
            metadata: HashMap::new(),
        }
    }
}

/// Resolved path helpers for session artifacts.
pub fn sessions_dir(project_root: &Path) -> PathBuf {
    project_root.join(".vac/sessions")
}

pub fn snapshot_path(project_root: &Path, session_id: Uuid) -> PathBuf {
    sessions_dir(project_root).join(format!("{session_id}.snapshot.json"))
}

pub fn checkpoint_dir(project_root: &Path) -> PathBuf {
    project_root.join(".vac/checkpoints")
}

/// Save a session snapshot to disk.
pub async fn save_snapshot_async(snapshot: SessionSnapshot) -> Result<PathBuf> {
    let dir = sessions_dir(&snapshot.project_root);
    fs::create_dir_all(&dir).await?;
    let path = snapshot_path(&snapshot.project_root, snapshot.session_id);
    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(&path, json).await?;
    tracing::debug!(session_id = %snapshot.session_id, "session snapshot saved");
    Ok(path)
}

/// Load a session snapshot from disk.
pub async fn load_snapshot_async(
    project_root: PathBuf,
    session_id: Uuid,
) -> Result<SessionSnapshot> {
    let path = snapshot_path(&project_root, session_id);
    if !fs::try_exists(&path).await? {
        return Err(SessionControlError::NotFound(session_id));
    }
    let content = fs::read_to_string(&path).await?;
    let snapshot: SessionSnapshot = serde_json::from_str(&content)?;
    validate_schema_version(snapshot.schema_version)?;
    Ok(snapshot)
}

/// List all session snapshots, sorted by updated_at descending.
pub async fn list_snapshots_async(project_root: PathBuf) -> Result<Vec<SessionSnapshot>> {
    let dir = sessions_dir(&project_root);
    if !fs::try_exists(&dir).await.unwrap_or(false) {
        return Ok(Vec::new());
    }
    let mut snapshots = Vec::new();
    let mut entries = fs::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && path.to_string_lossy().contains(".snapshot.")
        {
            if let Ok(content) = fs::read_to_string(&path).await {
                if let Ok(snapshot) = serde_json::from_str::<SessionSnapshot>(&content) {
                    snapshots.push(snapshot);
                }
            }
        }
    }
    snapshots.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(snapshots)
}

/// Check if a session has a checkpoint available.
pub async fn has_checkpoint_async(project_root: PathBuf, session_id: Uuid) -> bool {
    let cp_dir = checkpoint_dir(&project_root);
    let cp_path = cp_dir.join(format!("{session_id}_state.json"));
    fs::try_exists(&cp_path).await.unwrap_or(false)
}

/// Validate schema version compatibility.
fn validate_schema_version(version: u32) -> Result<()> {
    if version > CURRENT_SCHEMA_VERSION {
        return Err(SessionControlError::UnsupportedSchema {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    Ok(())
}

/// Migrate a snapshot from an older schema version to current.
pub fn migrate_snapshot(mut snapshot: SessionSnapshot) -> Result<SessionSnapshot> {
    match snapshot.schema_version {
        0 => {
            // v0 → v1: add default tui_state and metadata
            if snapshot.metadata.is_empty() {
                snapshot
                    .metadata
                    .insert("migrated_from".into(), "v0".into());
            }
            snapshot.schema_version = 1;
            migrate_snapshot(snapshot)
        }
        1 => {
            // v1 → v2: M9 bump
            snapshot.schema_version = 2;
            Ok(snapshot)
        }
        2 => Ok(snapshot),
        v => Err(SessionControlError::UnsupportedSchema {
            found: v,
            supported: CURRENT_SCHEMA_VERSION,
        }),
    }
}

/// Session cleanup: remove all artifacts for a session.
pub struct CleanupReport {
    pub snapshot_removed: bool,
    pub checkpoint_removed: bool,
    pub approvals_removed: usize,
    pub errors: Vec<String>,
}

/// Clean up all artifacts associated with a session.
pub async fn cleanup_session_async(
    project_root: PathBuf,
    session_id: Uuid,
) -> Result<CleanupReport> {
    let mut report = CleanupReport {
        snapshot_removed: false,
        checkpoint_removed: false,
        approvals_removed: 0,
        errors: Vec::new(),
    };

    // Remove snapshot
    let snap = snapshot_path(&project_root, session_id);
    if fs::try_exists(&snap).await.unwrap_or(false) {
        match fs::remove_file(&snap).await {
            Ok(()) => report.snapshot_removed = true,
            Err(e) => report.errors.push(format!("snapshot: {e}")),
        }
    }

    // Remove checkpoint state
    let cp = checkpoint_dir(&project_root).join(format!("{session_id}_state.json"));
    if fs::try_exists(&cp).await.unwrap_or(false) {
        match fs::remove_file(&cp).await {
            Ok(()) => report.checkpoint_removed = true,
            Err(e) => report.errors.push(format!("checkpoint: {e}")),
        }
    }

    // Remove approval records for this session
    let approvals_dir = project_root.join(".vac/approvals");
    if fs::try_exists(&approvals_dir).await.unwrap_or(false) {
        if let Ok(mut entries) = fs::read_dir(&approvals_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if let Ok(content) = fs::read_to_string(&path).await {
                    if content.contains(&session_id.to_string())
                        && fs::remove_file(&path).await.is_ok()
                    {
                        report.approvals_removed += 1;
                    }
                }
            }
        }
    }

    tracing::info!(
        session_id = %session_id,
        snapshot = report.snapshot_removed,
        checkpoint = report.checkpoint_removed,
        approvals = report.approvals_removed,
        errors = report.errors.len(),
        "session cleanup complete"
    );

    Ok(report)
}

/// Check if a session is stale (not updated within the given duration).
pub fn is_stale(snapshot: &SessionSnapshot, max_age: chrono::Duration) -> bool {
    let cutoff = Utc::now() - max_age;
    snapshot.updated_at < cutoff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let sid = Uuid::new_v4();

        let mut snap = SessionSnapshot::new(sid, root.clone());
        snap.active_model = Some("claude-opus-4".into());
        snap.task_count = 5;
        snap.completed_tasks = 3;
        snap.failed_tasks = 1;

        let path = save_snapshot_async(snap).await.unwrap();
        assert!(path.exists());

        let loaded = load_snapshot_async(root, sid).await.unwrap();
        assert_eq!(loaded.session_id, sid);
        assert_eq!(loaded.active_model.as_deref(), Some("claude-opus-4"));
        assert_eq!(loaded.task_count, 5);
        assert_eq!(loaded.completed_tasks, 3);
    }

    #[tokio::test]
    async fn list_snapshots_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();

        let mut s1 = SessionSnapshot::new(Uuid::new_v4(), root.clone());
        s1.updated_at = Utc::now() - chrono::Duration::hours(2);
        save_snapshot_async(s1).await.unwrap();

        let s2 = SessionSnapshot::new(Uuid::new_v4(), root.clone());
        save_snapshot_async(s2).await.unwrap();

        let list = list_snapshots_async(root).await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].updated_at >= list[1].updated_at);
    }

    #[tokio::test]
    async fn load_nonexistent_session_returns_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let err = load_snapshot_async(tmp.path().to_path_buf(), Uuid::new_v4())
            .await
            .unwrap_err();
        assert!(matches!(err, SessionControlError::NotFound(_)));
    }

    #[test]
    fn unsupported_schema_rejected() {
        let err = validate_schema_version(99).unwrap_err();
        assert!(matches!(err, SessionControlError::UnsupportedSchema { .. }));
    }

    #[test]
    fn migrate_v0_to_v1() {
        let mut snap = SessionSnapshot::new(Uuid::new_v4(), PathBuf::from("/tmp"));
        snap.schema_version = 0;
        let migrated = migrate_snapshot(snap).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.metadata.get("migrated_from").unwrap(), "v0");
    }

    #[tokio::test]
    async fn cleanup_removes_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let sid = Uuid::new_v4();

        let snap = SessionSnapshot::new(sid, root.clone());
        save_snapshot_async(snap).await.unwrap();

        let cp_dir = checkpoint_dir(&root);
        fs::create_dir_all(&cp_dir).await.unwrap();
        tokio::fs::write(cp_dir.join(format!("{sid}_state.json")), "{}")
            .await
            .unwrap();

        let report = cleanup_session_async(root, sid).await.unwrap();
        assert!(report.snapshot_removed);
        assert!(report.checkpoint_removed);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn stale_detection() {
        let mut snap = SessionSnapshot::new(Uuid::new_v4(), PathBuf::from("/tmp"));
        snap.updated_at = Utc::now() - chrono::Duration::days(30);
        assert!(is_stale(&snap, chrono::Duration::days(7)));
        assert!(!is_stale(&snap, chrono::Duration::days(60)));
    }

    #[tokio::test]
    async fn has_checkpoint_returns_false_for_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!has_checkpoint_async(tmp.path().to_path_buf(), Uuid::new_v4()).await);
    }
}
