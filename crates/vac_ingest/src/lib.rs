//! VAC project context ingestion.

pub mod bm25;
mod index;
mod pending;
mod root;
mod trajectory_bridge;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::warn;

pub use bm25::{Bm25Index, Bm25Params, RankedPath, rank_paths};
pub use index::build_file_index;
pub use pending::collect_pending_changes;
pub use root::detect_project_root;
pub use trajectory_bridge::collect_recent_trajectories;

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
    let root = detect_project_root(cwd)
        .await
        .unwrap_or_else(|| cwd.to_path_buf());
    let session_title = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some("workspace".to_string()));

    let (file_index_res, recent_trajectories_res, pending_changes_res) = tokio::join!(
        build_file_index(&root, 50_000),
        collect_recent_trajectories(&root, 5),
        collect_pending_changes(&root, 5)
    );

    let file_index = match file_index_res {
        Ok(files) => files,
        Err(err) => {
            warn!(error = %err, root = %root.display(), "project file index bootstrap failed");
            Vec::new()
        }
    };
    let recent_trajectories = match recent_trajectories_res {
        Ok(labels) => labels,
        Err(err) => {
            warn!(
                error = %err,
                root = %root.display(),
                "recent trajectory bootstrap failed"
            );
            Vec::new()
        }
    };
    let pending_changes = match pending_changes_res {
        Ok(changes) => changes,
        Err(err) => {
            warn!(error = %err, root = %root.display(), "pending change bootstrap failed");
            Vec::new()
        }
    };

    Ok(ProjectContext {
        root,
        session_title,
        file_index,
        recent_trajectories,
        pending_changes,
    })
}
