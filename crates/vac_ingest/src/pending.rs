use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub async fn collect_pending_changes(root: &Path, limit: usize) -> Result<Vec<String>> {
    let git_changes = collect_git_changes(root).await.unwrap_or_default();
    let session_changes = collect_session_changes(root).await.unwrap_or_default();

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

pub async fn collect_recent_trajectories(root: &Path, limit: usize) -> Result<Vec<String>> {
    let dir = root.join(".vac/sessions");
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(&dir).await?;
    let mut sessions: Vec<(std::time::SystemTime, String)> = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let metadata = entry.metadata().await.ok();
        let modified = metadata
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session")
            .to_string();
        sessions.push((modified, label));
    }

    sessions.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(sessions
        .into_iter()
        .take(limit)
        .map(|(_, label)| label)
        .collect())
}

async fn collect_git_changes(root: &Path) -> Result<Vec<String>> {
    let output = tokio::process::Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .arg("--untracked-files=all")
        .current_dir(root)
        .output()
        .await?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in stdout.lines() {
        let path = line.get(3..).unwrap_or(line).trim();
        if path.is_empty() {
            continue;
        }
        out.push(path.to_string());
    }
    Ok(out)
}

async fn collect_session_changes(root: &Path) -> Result<Vec<String>> {
    let session = latest_session_json(root).await?;
    let Some(session) = session else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    if let Some(results) = session.get("results").and_then(Value::as_object) {
        for result in results.values() {
            if let Some(files) = result.get("modified_files").and_then(Value::as_array) {
                for file in files {
                    if let Some(path) = file.as_str() {
                        out.push(path.to_string());
                    }
                }
            }
            if let Some(files) = result.get("created_files").and_then(Value::as_array) {
                for file in files {
                    if let Some(path) = file.as_str() {
                        out.push(path.to_string());
                    }
                }
            }
        }
    }
    Ok(out)
}

async fn latest_session_json(root: &Path) -> Result<Option<Value>> {
    let dir = root.join(".vac/sessions");
    if !dir.exists() {
        return Ok(None);
    }

    let mut entries = tokio::fs::read_dir(&dir).await?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let modified = entry
            .metadata()
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if newest
            .as_ref()
            .map(|(ts, _)| modified > *ts)
            .unwrap_or(true)
        {
            newest = Some((modified, path));
        }
    }

    let Some((_, path)) = newest else {
        return Ok(None);
    };
    let content = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&content).ok())
}
