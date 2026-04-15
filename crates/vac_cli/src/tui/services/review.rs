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
    let abs_path = if Path::new(_file_path).is_absolute() {
        PathBuf::from(_file_path)
    } else {
        _project_root.join(_file_path)
    };

    let snap_path = snapshot_path(_project_root, _session_id, _file_path);

    let old_content = if snap_path.exists() {
        std::fs::read_to_string(&snap_path)
            .map_err(|e| format!("Failed to read snapshot {}: {e}", snap_path.display()))?
    } else {
        String::new()
    };

    let new_content = if abs_path.exists() {
        std::fs::read_to_string(&abs_path)
            .map_err(|e| format!("Failed to read file {}: {e}", abs_path.display()))?
    } else {
        String::new()
    };

    Ok(DiffData {
        path: _file_path.to_string(),
        old_content,
        new_content,
    })
}
