use std::path::{Path, PathBuf};

/// Detect the most likely VAC project root from `cwd`.
pub fn detect_project_root(cwd: &Path) -> Option<PathBuf> {
    for ancestor in cwd.ancestors() {
        if ancestor.join(".vac/config.toml").exists() {
            return Some(ancestor.to_path_buf());
        }
    }

    for ancestor in cwd.ancestors() {
        if ancestor.join("vil.toml").exists() {
            return Some(ancestor.to_path_buf());
        }
    }

    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            return Some(ancestor.to_path_buf());
        }
    }

    None
}
