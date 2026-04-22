use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use tokio::fs;
use tracing::debug;

pub async fn collect_pending_changes(root: &Path, limit: usize) -> Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let (git_changes, session_changes) =
        tokio::join!(collect_git_changes(root), collect_session_changes(root));
    let git_changes = git_changes.unwrap_or_default();
    let session_changes = session_changes.unwrap_or_default();

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in git_changes.into_iter().chain(session_changes.into_iter()) {
        if seen.insert(item.clone()) {
            out.push(item);
        }
        if out.len() >= limit {
            break;
        }
    }

    Ok(out)
}

async fn collect_git_changes(root: &Path) -> Result<Vec<String>> {
    let output = match tokio::process::Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .arg("--untracked-files=all")
        .current_dir(root)
        .output()
        .await
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(root = %root.display(), "git binary unavailable while collecting pending changes");
            return Ok(Vec::new());
        }
        Err(err) => return Err(err.into()),
    };

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in stdout.lines() {
        let path = line.get(3..).unwrap_or(line).trim();
        let path = path.rsplit_once(" -> ").map(|(_, new)| new).unwrap_or(path);
        if let Some(path) = normalize_pending_path(root, path) {
            out.push(path);
        }
    }
    Ok(out)
}

async fn collect_session_changes(root: &Path) -> Result<Vec<String>> {
    let Some(session) = latest_session_json(root).await? else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    if let Some(results) = session.get("results").and_then(Value::as_object) {
        for result in results.values() {
            collect_paths_from_value(root, result.get("modified_files"), &mut out);
            collect_paths_from_value(root, result.get("created_files"), &mut out);
            collect_paths_from_value(root, result.get("deleted_files"), &mut out);
        }
    }
    Ok(out)
}

async fn latest_session_json(root: &Path) -> Result<Option<Value>> {
    let dir = root.join(".vac/sessions");
    if !fs::try_exists(&dir).await.unwrap_or(false) {
        return Ok(None);
    }

    let mut entries = match fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(err) => {
            debug!(
                error = %err,
                root = %root.display(),
                "unable to read sessions directory while collecting pending changes"
            );
            return Ok(None);
        }
    };

    let mut sessions: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    while let Some(entry) = match entries.next_entry().await {
        Ok(entry) => entry,
        Err(err) => {
            debug!(
                error = %err,
                root = %root.display(),
                "unable to iterate session snapshots while collecting pending changes"
            );
            return Ok(None);
        }
    } {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let modified = entry
            .metadata()
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        sessions.push((modified, path));
    }

    sessions.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in sessions {
        let content = match fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(err) => {
                debug!(error = %err, path = %path.display(), "unable to read session snapshot");
                continue;
            }
        };

        match serde_json::from_str::<Value>(&content) {
            Ok(value) => return Ok(Some(value)),
            Err(err) => {
                debug!(error = %err, path = %path.display(), "unable to parse session snapshot");
            }
        }
    }

    Ok(None)
}

fn collect_paths_from_value(root: &Path, value: Option<&Value>, out: &mut Vec<String>) {
    let Some(value) = value else {
        return;
    };

    let Some(items) = value.as_array() else {
        return;
    };

    for item in items {
        if let Some(raw) = extract_path_like_string(item) {
            if let Some(normalized) = normalize_pending_path(root, raw) {
                out.push(normalized);
            }
        }
    }
}

fn extract_path_like_string(value: &Value) -> Option<&str> {
    if let Some(s) = value.as_str() {
        return Some(s);
    }

    let object = value.as_object()?;
    for key in ["path", "file", "name"] {
        if let Some(s) = object.get(key).and_then(Value::as_str) {
            return Some(s);
        }
    }
    None
}

fn normalize_pending_path(root: &Path, raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let raw = trimmed.replace('\\', "/");
    let path = Path::new(&raw);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }

    let rel = if path.is_absolute() {
        path.strip_prefix(root).ok()?
    } else {
        path
    };

    let normalized = rel.to_string_lossy().replace('\\', "/");
    let normalized = normalized
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string();
    if normalized.is_empty() || is_internal_pending_path(&normalized) {
        return None;
    }

    Some(normalized)
}

fn is_internal_pending_path(path: &str) -> bool {
    path == ".vac"
        || path.starts_with(".vac/")
        || path == ".git"
        || path.starts_with(".git/")
        || path == "target"
        || path.starts_with("target/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn session_changes_are_normalized_and_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let session_dir = root.join(".vac/sessions");
        fs::create_dir_all(&session_dir).await.unwrap();
        fs::write(
            session_dir.join("latest.json"),
            r#"{
                "results": {
                    "task": {
                        "modified_files": ["src/main.rs", "./src/lib.rs"],
                        "created_files": [{"path": "src/new.rs"}],
                        "deleted_files": ["target/cache.bin"]
                    }
                }
            }"#,
        )
        .await
        .unwrap();

        let changes = collect_session_changes(root).await.unwrap();
        assert!(changes.contains(&"src/main.rs".to_string()));
        assert!(changes.contains(&"src/lib.rs".to_string()));
        assert!(changes.contains(&"src/new.rs".to_string()));
        assert!(!changes.iter().any(|path| path.starts_with("target/")));
    }
}
