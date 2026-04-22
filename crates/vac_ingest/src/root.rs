use std::path::{Path, PathBuf};
use tokio::fs;

/// Detect the most likely VAC project root from `cwd`.
///
/// Priority is:
/// 1. `.vac/config.toml`
/// 2. `vil.toml`
/// 3. VWFD documents under `workflows/` or `.vac/workflows/`
/// 4. `.git`
pub async fn detect_project_root(cwd: impl AsRef<Path>) -> Option<PathBuf> {
    let cwd = cwd.as_ref();

    for ancestor in cwd.ancestors() {
        if has_path(ancestor, ".vac/config.toml").await {
            return Some(ancestor.to_path_buf());
        }
    }

    for ancestor in cwd.ancestors() {
        if has_path(ancestor, "vil.toml").await {
            return Some(ancestor.to_path_buf());
        }
    }

    for ancestor in cwd.ancestors() {
        if has_vwfd_documents(ancestor).await {
            return Some(ancestor.to_path_buf());
        }
    }

    for ancestor in cwd.ancestors() {
        if has_path(ancestor, ".git").await {
            return Some(ancestor.to_path_buf());
        }
    }

    None
}

async fn has_path(root: &Path, rel: &str) -> bool {
    fs::try_exists(root.join(rel)).await.unwrap_or(false)
}

async fn has_vwfd_documents(root: &Path) -> bool {
    for rel in ["workflows", ".vac/workflows"] {
        if dir_contains_vwfd(&root.join(rel)).await {
            return true;
        }
    }
    false
}

async fn dir_contains_vwfd(dir: &Path) -> bool {
    if !fs::try_exists(dir).await.unwrap_or(false) {
        return false;
    }

    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = fs::read_dir(&current).await else {
            continue;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if file_type.is_file() && is_vwfd_path(&path) {
                return true;
            }
        }
    }

    false
}

fn is_vwfd_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    name.ends_with(".vwfd.yaml") || name.ends_with(".vwfd.yml")
}
