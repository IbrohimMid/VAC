use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub async fn build_file_index(root: &Path, limit: usize) -> Result<Vec<PathBuf>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || scan_file_index(&root, limit)).await?
}

fn scan_file_index(root: &Path, limit: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();

    let walker = WalkBuilder::new(root)
        .add_custom_ignore_filename(".gitignore")
        .git_ignore(true)
        .git_exclude(true)
        .hidden(true)
        .build();

    for entry in walker {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        if rel.starts_with(".vac") || rel.starts_with(".git") || rel.starts_with("target") {
            continue;
        }
        if seen.insert(rel.clone()) {
            files.push(rel);
        }
        if files.len() >= limit {
            break;
        }
    }

    files.sort();
    Ok(files)
}
