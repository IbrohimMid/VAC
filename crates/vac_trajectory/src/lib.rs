//! VAC trajectory analysis.
//!
//! This crate reads trace/session artifacts written under `.vac/` and turns
//! them into concise, user-facing summaries for `vac observe`, `vac explain`,
//! and `vac why`.

pub mod decisions;
mod model;
mod scan;
mod session;
mod trace;

use anyhow::Result;
use std::path::Path;

pub use model::{
    ArtifactSource, ExplainReport, FileMatchReason, FileWhyReport, TrajectoryArtifact,
    TrajectoryEvidence, TrajectoryStatus,
};

/// Collect recent trajectory artifacts from a project root.
pub async fn collect_recent_trajectories(
    root: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<TrajectoryArtifact>> {
    scan::collect_recent_trajectories(root.as_ref(), limit).await
}

/// Collect recent trajectory labels, preserving the legacy ingest contract.
pub async fn collect_recent_labels(root: impl AsRef<Path>, limit: usize) -> Result<Vec<String>> {
    let artifacts = collect_recent_trajectories(root, limit).await?;
    Ok(dedup_labels(
        artifacts.into_iter().map(|artifact| artifact.short_label),
    ))
}

/// Explain the most relevant trajectory for an optional selector.
pub async fn explain_trajectory(
    root: impl AsRef<Path>,
    selector: Option<&str>,
) -> Result<Option<ExplainReport>> {
    scan::explain_trajectory(root.as_ref(), selector).await
}

/// Explain why a file changed, based on the most relevant trajectory.
pub async fn why_file_changed(
    root: impl AsRef<Path>,
    file: impl AsRef<Path>,
    selector: Option<&str>,
) -> Result<Option<FileWhyReport>> {
    scan::why_file_changed(root.as_ref(), file.as_ref(), selector).await
}

fn dedup_labels<I>(labels: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for label in labels {
        if seen.insert(label.clone()) {
            deduped.push(label);
        }
    }
    deduped
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn trace_record(
        ts: i64,
        record_type: vac_trace::RecordType,
        content: serde_json::Value,
    ) -> vac_trace::recorder::TraceRecord {
        vac_trace::recorder::TraceRecord {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc.timestamp_opt(ts, 0).unwrap(),
            record_type,
            agent_id: None,
            content,
        }
    }

    #[tokio::test]
    async fn collect_recent_labels_dedupes_and_preserves_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".vac/traces")).unwrap();
        std::fs::create_dir_all(root.join(".vac/sessions")).unwrap();
        std::fs::write(root.join(".vac/traces/alpha.json"), "[]").unwrap();
        std::fs::write(root.join(".vac/sessions/alpha.json"), "{}").unwrap();
        std::fs::write(root.join(".vac/sessions/beta.json"), "{}").unwrap();

        let labels = collect_recent_labels(root, 10).await.unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "alpha");
        assert_eq!(labels[1], "beta");
    }

    #[tokio::test]
    async fn explain_trace_and_session_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".vac/traces")).unwrap();
        std::fs::create_dir_all(root.join(".vac/sessions")).unwrap();

        let trace = vec![
            trace_record(
                1,
                vac_trace::RecordType::TaskStart,
                json!({"task_id": "task-1", "description": "investigate parser"}),
            ),
            trace_record(
                2,
                vac_trace::RecordType::TaskComplete,
                json!({"task_id": "task-1", "summary": "completed parser fix"}),
            ),
        ];
        std::fs::write(
            root.join(".vac/traces/trace-a.json"),
            serde_json::to_string_pretty(&trace).unwrap(),
        )
        .unwrap();

        let session = json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "created_at": "2026-04-21T00:00:00Z",
            "updated_at": "2026-04-22T00:00:00Z",
            "tasks": [
                {
                    "id": "11111111-1111-1111-1111-111111111111",
                    "description": "edit docs",
                    "constraints": { "target_paths": ["src/lib.rs"] }
                }
            ],
            "results": {
                "11111111-1111-1111-1111-111111111111": {
                    "summary": "updated docs",
                    "modified_files": ["src/lib.rs"],
                    "created_files": [],
                    "total_tokens_used": 42,
                    "elapsed_ms": 10
                }
            },
            "metadata": {
                "total_tokens_used": 42,
                "total_tasks_completed": 1,
                "total_tasks_failed": 0,
                "total_files_modified": 1
            }
        });
        std::fs::write(
            root.join(".vac/sessions/session-a.json"),
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        let explanations = collect_recent_trajectories(root, 10).await.unwrap();
        assert!(
            explanations
                .iter()
                .any(|artifact| artifact.short_label == "trace-a")
        );
        assert!(
            explanations
                .iter()
                .any(|artifact| artifact.short_label == "session-a")
        );

        let explain = explain_trajectory(root, Some("session-a"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(explain.artifact.short_label, "session-a");
        assert!(
            explain
                .artifact
                .modified_files
                .contains(&"src/lib.rs".to_string())
        );

        let why = why_file_changed(root, "src/lib.rs", None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(why.path, "src/lib.rs");
        assert_eq!(why.matches[0].match_reason, FileMatchReason::ModifiedFile);
        assert!(why.matches[0].artifact.short_label.contains("session-a"));
    }
}
