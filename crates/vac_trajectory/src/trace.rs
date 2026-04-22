use crate::model::{ArtifactSource, TrajectoryArtifact, TrajectoryEvidence, TrajectoryStatus};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::Path;
use vac_trace::{RecordType, recorder::TraceRecord};

#[derive(Debug, Clone, Default, Deserialize)]
struct TraceTaskStart {
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TraceTaskComplete {
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    summary: String,
}

pub async fn summarize_trace_file(
    path: &Path,
    modified_at: Option<DateTime<Utc>>,
) -> Result<TrajectoryArtifact> {
    let content = tokio::fs::read_to_string(path).await?;
    let records: Vec<TraceRecord> = serde_json::from_str(&content)?;
    Ok(summarize_trace_records(path, records, modified_at))
}

pub fn summarize_trace_records(
    path: &Path,
    records: Vec<TraceRecord>,
    modified_at: Option<DateTime<Utc>>,
) -> TrajectoryArtifact {
    let short_label = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("trace")
        .to_string();

    let mut started_at = records.first().map(|record| record.timestamp);
    let mut updated_at = records
        .last()
        .map(|record| record.timestamp)
        .or(modified_at);
    let mut task_description = None;
    let mut summary = None;
    let mut task_count = 0usize;
    let mut tool_calls = 0usize;
    let mut tool_results = 0usize;
    let mut policy_decisions = 0usize;
    let mut saw_failure = false;
    let mut target_paths = Vec::new();
    let mut target_modules = Vec::new();
    let mut evidence = Vec::new();

    for record in &records {
        started_at = Some(started_at.map_or(record.timestamp, |ts| ts.min(record.timestamp)));
        updated_at = Some(updated_at.map_or(record.timestamp, |ts| ts.max(record.timestamp)));
        match record.record_type {
            RecordType::TaskStart => {
                task_count += 1;
                if let Ok(task) = serde_json::from_value::<TraceTaskStart>(record.content.clone()) {
                    if task_description.is_none() && !task.description.trim().is_empty() {
                        task_description = Some(task.description.clone());
                    }
                    if !task.task_id.is_empty() {
                        evidence.push(TrajectoryEvidence {
                            label: "task start".to_string(),
                            detail: format!("{}: {}", task.task_id, task.description),
                        });
                    }
                }
            }
            RecordType::TaskComplete => {
                if let Ok(task) =
                    serde_json::from_value::<TraceTaskComplete>(record.content.clone())
                {
                    if summary.is_none() && !task.summary.trim().is_empty() {
                        summary = Some(task.summary.clone());
                    }
                    if !task.task_id.is_empty() {
                        evidence.push(TrajectoryEvidence {
                            label: "task complete".to_string(),
                            detail: format!("{}: {}", task.task_id, task.summary),
                        });
                    }
                }
            }
            RecordType::TaskFailed => {
                saw_failure = true;
                if summary.is_none() {
                    summary = Some(
                        record
                            .content
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("task failed")
                            .to_string(),
                    );
                }
                evidence.push(TrajectoryEvidence {
                    label: "task failed".to_string(),
                    detail: record
                        .content
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("task failed")
                        .to_string(),
                });
            }
            RecordType::ToolCall => {
                tool_calls += 1;
                if let Some(tool_name) = record.content.get("tool").and_then(|v| v.as_str()) {
                    let mut snippets = collect_strings(&record.content);
                    snippets.retain(|snippet| !snippet.trim().is_empty());
                    snippets.retain(|snippet| snippet.len() <= 200);
                    if !snippets.is_empty() {
                        snippets.sort();
                        snippets.dedup();
                        for snippet in &snippets {
                            if path_like(snippet) {
                                target_paths.push(snippet.clone());
                            }
                            if module_like(snippet) {
                                target_modules.push(snippet.clone());
                            }
                        }
                        evidence.push(TrajectoryEvidence {
                            label: format!("tool call: {tool_name}"),
                            detail: snippets.join(", "),
                        });
                    } else {
                        evidence.push(TrajectoryEvidence {
                            label: format!("tool call: {tool_name}"),
                            detail: "no structured arguments captured".to_string(),
                        });
                    }
                }
            }
            RecordType::ToolResult => {
                tool_results += 1;
                if let Some(tool_name) = record.content.get("tool").and_then(|v| v.as_str()) {
                    let mut snippets = collect_strings(&record.content);
                    snippets.retain(|snippet| !snippet.trim().is_empty());
                    snippets.retain(|snippet| snippet.len() <= 200);
                    if !snippets.is_empty() {
                        snippets.sort();
                        snippets.dedup();
                        evidence.push(TrajectoryEvidence {
                            label: format!("tool result: {tool_name}"),
                            detail: snippets.join(", "),
                        });
                    }
                }
            }
            RecordType::PolicyDecision => {
                policy_decisions += 1;
                if let Some(tool) = record.content.get("tool").and_then(|v| v.as_str()) {
                    evidence.push(TrajectoryEvidence {
                        label: "policy decision".to_string(),
                        detail: format!(
                            "{}: {}",
                            tool,
                            record
                                .content
                                .get("decision")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        ),
                    });
                }
            }
            _ => {}
        }
    }

    let status = if saw_failure {
        TrajectoryStatus::Failed
    } else if summary.is_some() {
        TrajectoryStatus::Completed
    } else if records.is_empty() {
        TrajectoryStatus::Unknown
    } else {
        TrajectoryStatus::Partial
    };

    let summary = summary.or_else(|| {
        if tool_calls > 0 || tool_results > 0 || policy_decisions > 0 {
            Some(format!(
                "{tool_calls} tool calls, {tool_results} tool results, {policy_decisions} policy decisions"
            ))
        } else {
            None
        }
    });

    if task_count > 0 {
        evidence.push(TrajectoryEvidence {
            label: "task count".to_string(),
            detail: format!("{task_count} task start record(s)"),
        });
    }

    target_paths.sort();
    target_paths.dedup();
    target_modules.sort();
    target_modules.dedup();

    TrajectoryArtifact {
        source: ArtifactSource::Trace,
        short_label,
        artifact_path: path.to_path_buf(),
        title: task_description
            .clone()
            .unwrap_or_else(|| "trace snapshot".to_string()),
        status,
        started_at,
        updated_at,
        task_description,
        summary,
        modified_files: Vec::new(),
        created_files: Vec::new(),
        target_paths,
        target_modules,
        total_tokens_used: None,
        task_count,
        evidence,
    }
}

fn collect_strings(value: &serde_json::Value) -> Vec<String> {
    fn walk(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => {
                if !s.is_empty() && s.len() <= 200 {
                    out.push(s.clone());
                }
            }
            serde_json::Value::Array(items) => {
                for item in items.iter().take(32) {
                    walk(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if matches!(key.as_str(), "content" | "message" | "summary") {
                        if let Some(s) = value.as_str() {
                            if s.len() <= 200 {
                                out.push(s.to_string());
                            }
                        }
                        continue;
                    }
                    walk(value, out);
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    walk(value, &mut out);
    out
}

fn path_like(snippet: &str) -> bool {
    snippet.contains('/')
        || snippet.contains('\\')
        || snippet.ends_with(".rs")
        || snippet.ends_with(".json")
}

fn module_like(snippet: &str) -> bool {
    snippet.contains("::") || snippet.contains("mod ")
}
