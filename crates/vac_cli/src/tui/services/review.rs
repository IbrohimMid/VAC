use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiffData {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
}

pub fn snapshot_path(project_root: &Path, session_id: Uuid, file_path: &str) -> PathBuf {
    let safe_name = format!("{}.bak", file_path.replace(['/', '\\'], "__"));
    project_root
        .join(".vac/backups")
        .join(session_id.to_string())
        .join(safe_name)
}

pub fn load_diff(_project_root: &Path, _session_id: Uuid, _file_path: &str) -> Result<DiffData, String> {
    Err("not implemented".to_string())
}

