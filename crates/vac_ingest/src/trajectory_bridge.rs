use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use tokio::fs;

pub async fn collect_recent_trajectories(root: &Path, limit: usize) -> Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for (dir, source_priority) in [root.join(".vac/trajectories"), root.join(".vac/sessions")]
        .into_iter()
        .zip([0u8, 1u8])
    {
        if !fs::try_exists(&dir).await.unwrap_or(false) {
            continue;
        }
        if let Err(err) = collect_json_labels(&dir, source_priority, &mut sessions).await {
            tracing::debug!(error = %err, path = %dir.display(), "unable to scan trajectory directory");
        }
    }

    sessions.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let mut seen = HashSet::new();
    Ok(sessions
        .into_iter()
        .filter_map(|(_, _, label)| {
            if seen.insert(label.clone()) {
                Some(label)
            } else {
                None
            }
        })
        .take(limit)
        .collect())
}

async fn collect_json_labels(
    dir: &Path,
    source_priority: u8,
    out: &mut Vec<(std::time::SystemTime, u8, String)>,
) -> Result<()> {
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = match entries.next_entry().await {
        Ok(entry) => entry,
        Err(err) => {
            return Err(err.into());
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
        let label = path_to_label(&path);
        out.push((modified, source_priority, label));
    }
    Ok(())
}

fn path_to_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("trajectory")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prefers_newest_trajectory_labels_and_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let trajectories = root.join(".vac/trajectories");
        let sessions = root.join(".vac/sessions");
        fs::create_dir_all(&trajectories).await.unwrap();
        fs::create_dir_all(&sessions).await.unwrap();
        fs::write(trajectories.join("alpha.json"), "{}")
            .await
            .unwrap();
        fs::write(sessions.join("alpha.json"), "{}").await.unwrap();
        fs::write(sessions.join("beta.json"), "{}").await.unwrap();

        let labels = collect_recent_trajectories(root, 10).await.unwrap();
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"alpha".to_string()));
        assert!(labels.contains(&"beta".to_string()));
    }
}
