//! VAC project context ingestion.

mod index;
mod pending;
mod root;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub use index::build_file_index;
pub use pending::collect_pending_changes;
pub use root::detect_project_root;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub session_title: Option<String>,
    pub file_index: Vec<PathBuf>,
    pub recent_trajectories: Vec<String>,
    pub pending_changes: Vec<String>,
}

pub async fn bootstrap(cwd: impl AsRef<Path>) -> Result<ProjectContext> {
    let cwd = cwd.as_ref();
    let root = detect_project_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let session_title = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some("workspace".to_string()));

    let file_index = build_file_index(&root, 50_000).await.unwrap_or_default();
    let recent_trajectories = pending::collect_recent_trajectories(&root, 5)
        .await
        .unwrap_or_default();
    let pending_changes = collect_pending_changes(&root, 5).await.unwrap_or_default();

    Ok(ProjectContext {
        root,
        session_title,
        file_index,
        recent_trajectories,
        pending_changes,
    })
}
