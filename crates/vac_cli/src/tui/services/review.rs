use ratatui::text::Line;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiffData {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<String>,
    pub staged: bool,
}

#[derive(Debug, Clone)]
pub struct PatchModel {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

impl PatchModel {
    pub fn stage_hunk(&mut self, index: usize) {
        if let Some(hunk) = self.hunks.get_mut(index) {
            hunk.staged = true;
        }
    }

    pub fn unstage_hunk(&mut self, index: usize) {
        if let Some(hunk) = self.hunks.get_mut(index) {
            hunk.staged = false;
        }
    }

    pub fn revert_hunk(&mut self, index: usize, original_content: &str) -> String {
        // Simple partial revert placeholder: 
        // in a real implementation this would apply the reverse of the hunk patch to the current content.
        // For now, it marks the hunk as removed or returns the reconstructed string.
        if let Some(hunk) = self.hunks.get_mut(index) {
            hunk.staged = false; // logic to revert
        }
        original_content.to_string()
    }
}

pub fn snapshot_path(project_root: &Path, session_id: Uuid, file_path: &str) -> PathBuf {
    let safe_name = format!("{}.bak", file_path.replace(['/', '\\'], "__"));
    project_root
        .join(".vac/backups")
        .join(session_id.to_string())
        .join(safe_name)
}

pub fn load_diff(
    _project_root: &Path,
    _session_id: Uuid,
    _file_path: &str,
) -> Result<DiffData, String> {
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

pub fn detect_editor(preferred: Option<String>) -> Option<String> {
    if let Some(editor) = preferred
        && is_editor_available(&editor)
    {
        return Some(editor);
    }

    for editor in ["nvim", "vim", "nano"] {
        if is_editor_available(editor) {
            return Some(editor.to_string());
        }
    }

    None
}

fn is_editor_available(editor: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(editor)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn render_diff_viewport(
    old_content: &str,
    new_content: &str,
    max_width: usize,
    scroll: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let lines = crate::tui::services::file_diff::render_diff(old_content, new_content, max_width);
    if height == 0 {
        return vec![];
    }
    lines.into_iter().skip(scroll).take(height).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_diff_falls_back_to_empty_when_snapshot_missing() {
        let dir = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        std::fs::write(dir.path().join("a.txt"), "new").unwrap();
        let diff = load_diff(dir.path(), session_id, "a.txt").unwrap();
        assert_eq!(diff.old_content, "");
        assert_eq!(diff.new_content, "new");
    }

    #[test]
    fn load_diff_reads_snapshot_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(dir.path().join(".vac/backups").join(session_id.to_string()))
            .unwrap();
        std::fs::write(snapshot_path(dir.path(), session_id, "a.txt"), "old").unwrap();
        std::fs::write(dir.path().join("a.txt"), "new").unwrap();
        let diff = load_diff(dir.path(), session_id, "a.txt").unwrap();
        assert_eq!(diff.old_content, "old");
        assert_eq!(diff.new_content, "new");
    }

    #[test]
    fn render_diff_viewport_applies_scroll() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nX\nd\ne\n";
        let lines0 = render_diff_viewport(old, new, 120, 0, 3);
        let lines1 = render_diff_viewport(old, new, 120, 1, 3);
        assert_ne!(format!("{:?}", lines0), format!("{:?}", lines1));
    }
}
