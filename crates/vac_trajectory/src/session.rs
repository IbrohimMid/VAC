use crate::model::{ArtifactSource, TrajectoryArtifact, TrajectoryEvidence, TrajectoryStatus};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionArtifact {
    #[serde(default)]
    pub id: Option<Uuid>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub tasks: Vec<SessionTask>,
    #[serde(default)]
    pub results: HashMap<String, SessionTaskResult>,
    #[serde(default)]
    pub metadata: SessionMetadata,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionMetadata {
    #[serde(default)]
    pub total_tokens_used: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionTask {
    #[serde(default)]
    pub id: Option<Uuid>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub constraints: SessionTaskConstraints,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionTaskConstraints {
    #[serde(default)]
    pub target_paths: Vec<String>,
    #[serde(default)]
    pub target_modules: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionTaskResult {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub created_files: Vec<String>,
    #[serde(default)]
    pub total_tokens_used: Option<u64>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
}

pub async fn summarize_session_file(
    path: &Path,
    modified_at: Option<DateTime<Utc>>,
) -> Result<TrajectoryArtifact> {
    let content = tokio::fs::read_to_string(path).await?;
    let parsed: SessionArtifact = serde_json::from_str(&content)?;
    Ok(summarize_session_artifact(path, parsed, modified_at))
}

pub fn summarize_session_artifact(
    path: &Path,
    parsed: SessionArtifact,
    modified_at: Option<DateTime<Utc>>,
) -> TrajectoryArtifact {
    let short_label = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("session")
        .to_string();

    let result_task_id = parsed
        .tasks
        .iter()
        .rev()
        .filter_map(|task| task.id)
        .find(|task_id| parsed.results.contains_key(&task_id.to_string()));

    let task_description = result_task_id
        .and_then(|task_id| {
            parsed
                .tasks
                .iter()
                .rev()
                .find(|task| task.id == Some(task_id))
                .map(|task| task.description.clone())
        })
        .filter(|description| !description.trim().is_empty());

    let task_result = result_task_id
        .and_then(|task_id| parsed.results.get(&task_id.to_string()))
        .or_else(|| {
            parsed.results.values().find(|result| {
                !result.summary.trim().is_empty()
                    || !result.modified_files.is_empty()
                    || !result.created_files.is_empty()
            })
        });

    let mut modified_files = Vec::new();
    let mut created_files = Vec::new();
    let mut target_paths = Vec::new();
    let mut target_modules = Vec::new();
    let mut evidence = Vec::new();

    for task in &parsed.tasks {
        if !task.constraints.target_paths.is_empty() {
            target_paths.extend(task.constraints.target_paths.iter().cloned());
            evidence.push(TrajectoryEvidence {
                label: "task constraints".to_string(),
                detail: format!("target paths: {}", task.constraints.target_paths.join(", ")),
            });
        }
        if !task.constraints.target_modules.is_empty() {
            target_modules.extend(task.constraints.target_modules.iter().cloned());
            evidence.push(TrajectoryEvidence {
                label: "task constraints".to_string(),
                detail: format!(
                    "target modules: {}",
                    task.constraints.target_modules.join(", ")
                ),
            });
        }
    }

    if let Some(id) = parsed.id {
        evidence.push(TrajectoryEvidence {
            label: "session id".to_string(),
            detail: id.to_string(),
        });
    }

    for result in parsed.results.values() {
        modified_files.extend(result.modified_files.iter().cloned());
        created_files.extend(result.created_files.iter().cloned());
        if !result.summary.trim().is_empty() {
            evidence.push(TrajectoryEvidence {
                label: "task result".to_string(),
                detail: result.summary.clone(),
            });
        }
        if let Some(tokens) = result.total_tokens_used {
            if tokens > 0 {
                evidence.push(TrajectoryEvidence {
                    label: "task result".to_string(),
                    detail: format!("tokens used: {tokens}"),
                });
            }
        }
        if let Some(elapsed) = result.elapsed_ms {
            evidence.push(TrajectoryEvidence {
                label: "task result".to_string(),
                detail: format!("elapsed: {elapsed}ms"),
            });
        }
    }

    modified_files.sort();
    modified_files.dedup();
    created_files.sort();
    created_files.dedup();
    target_paths.sort();
    target_paths.dedup();
    target_modules.sort();
    target_modules.dedup();

    let summary = task_result
        .and_then(|result| (!result.summary.trim().is_empty()).then(|| result.summary.clone()))
        .or_else(|| {
            if !modified_files.is_empty() || !created_files.is_empty() {
                Some(format!(
                    "{} modified, {} created",
                    modified_files.len(),
                    created_files.len()
                ))
            } else {
                None
            }
        });

    let status = if !modified_files.is_empty() || !created_files.is_empty() {
        TrajectoryStatus::Completed
    } else if parsed.results.is_empty() {
        TrajectoryStatus::Unknown
    } else {
        TrajectoryStatus::Partial
    };

    TrajectoryArtifact {
        source: ArtifactSource::Session,
        short_label,
        artifact_path: path.to_path_buf(),
        title: task_description
            .clone()
            .unwrap_or_else(|| "session snapshot".to_string()),
        status,
        started_at: parsed.created_at.or(modified_at),
        updated_at: parsed.updated_at.or(modified_at),
        task_description,
        summary,
        modified_files,
        created_files,
        target_paths,
        target_modules,
        total_tokens_used: Some(parsed.metadata.total_tokens_used).filter(|tokens| *tokens > 0),
        task_count: parsed.tasks.len(),
        evidence,
    }
}
