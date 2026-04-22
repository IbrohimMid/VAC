use crate::model::{
    ArtifactSource, ExplainReport, FileMatch, FileMatchReason, FileWhyReport, TrajectoryArtifact,
    TrajectoryStatus,
};
use crate::session::summarize_session_file;
use crate::trace::summarize_trace_file;
use anyhow::Result;
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use std::path::Path;
use tokio::fs;
use tracing::warn;

const SEARCH_DIRS: &[(&str, ArtifactSource)] = &[
    (".vac/traces", ArtifactSource::Trace),
    (".vac/sessions", ArtifactSource::Session),
    (".vac/trajectories", ArtifactSource::Legacy),
];

pub async fn collect_recent_trajectories(
    root: &Path,
    limit: usize,
) -> Result<Vec<TrajectoryArtifact>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let mut artifacts = Vec::new();
    for (rel, source) in SEARCH_DIRS {
        match collect_dir(root, rel, *source).await {
            Ok(mut items) => artifacts.append(&mut items),
            Err(err) => {
                warn!(error = %err, path = %root.join(rel).display(), "unable to scan trajectory directory");
            }
        }
    }

    artifacts.sort_by(|left, right| {
        right
            .updated_at
            .unwrap_or_else(epoch)
            .cmp(&left.updated_at.unwrap_or_else(epoch))
            .then_with(|| {
                right
                    .started_at
                    .unwrap_or_else(epoch)
                    .cmp(&left.started_at.unwrap_or_else(epoch))
            })
            .then_with(|| left.short_label.cmp(&right.short_label))
            .then_with(|| left.artifact_path.cmp(&right.artifact_path))
    });

    Ok(artifacts.into_iter().take(limit).collect())
}

pub async fn explain_trajectory(
    root: &Path,
    selector: Option<&str>,
) -> Result<Option<ExplainReport>> {
    let artifacts = collect_recent_trajectories(root, usize::MAX).await?;
    let selected = select_artifact(&artifacts, selector);
    Ok(selected.map(|artifact| ExplainReport { artifact }))
}

pub async fn why_file_changed(
    root: &Path,
    file: &Path,
    selector: Option<&str>,
) -> Result<Option<FileWhyReport>> {
    let normalized_path = normalize_query_path(root, file);
    let artifacts = collect_recent_trajectories(root, usize::MAX).await?;
    let mut matches = Vec::new();
    let candidates: Vec<TrajectoryArtifact> = match selector {
        Some(selector) => artifacts
            .into_iter()
            .filter(|artifact| matches_selector(artifact, selector))
            .collect(),
        None => artifacts,
    };

    if candidates.is_empty() {
        return Ok(None);
    }

    for artifact in candidates {
        for candidate in artifact
            .modified_files
            .iter()
            .filter(|candidate| path_matches(candidate, &normalized_path))
        {
            matches.push(FileMatch {
                match_reason: FileMatchReason::ModifiedFile,
                detail: format!("listed in modified_files as {candidate}"),
                artifact: artifact.clone(),
            });
        }
        for candidate in artifact
            .created_files
            .iter()
            .filter(|candidate| path_matches(candidate, &normalized_path))
        {
            matches.push(FileMatch {
                match_reason: FileMatchReason::CreatedFile,
                detail: format!("listed in created_files as {candidate}"),
                artifact: artifact.clone(),
            });
        }
        for candidate in artifact
            .target_paths
            .iter()
            .filter(|candidate| path_matches(candidate, &normalized_path))
        {
            matches.push(FileMatch {
                match_reason: FileMatchReason::TargetPath,
                detail: format!("task constraint target_paths includes {candidate}"),
                artifact: artifact.clone(),
            });
        }
        for candidate in artifact.target_modules.iter().filter(|candidate| {
            let module = normalized_path_to_module(&normalized_path);
            !module.is_empty() && candidate.as_str() == module.as_str()
        }) {
            matches.push(FileMatch {
                match_reason: FileMatchReason::TargetModule,
                detail: format!("task constraint target_modules includes {candidate}"),
                artifact: artifact.clone(),
            });
        }
        for item in artifact.evidence.iter().filter(|item| {
            item.detail.contains(&normalized_path) || item.label.contains(&normalized_path)
        }) {
            matches.push(FileMatch {
                match_reason: FileMatchReason::TraceArgument,
                detail: format!("evidence: {} — {}", item.label, item.detail),
                artifact: artifact.clone(),
            });
        }
    }

    matches.sort_by(|left, right| {
        match_priority(left.match_reason)
            .cmp(&match_priority(right.match_reason))
            .then_with(|| {
                right
                    .artifact
                    .updated_at
                    .unwrap_or_else(epoch)
                    .cmp(&left.artifact.updated_at.unwrap_or_else(epoch))
            })
    });

    Ok(Some(FileWhyReport {
        path: file.display().to_string(),
        normalized_path,
        matches,
    }))
}

async fn collect_dir(
    root: &Path,
    rel: &str,
    source: ArtifactSource,
) -> Result<Vec<TrajectoryArtifact>> {
    let dir = root.join(rel);
    if !fs::try_exists(&dir).await.unwrap_or(false) {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut entries = fs::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let metadata = entry.metadata().await.ok();
        let modified_at = metadata
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .map(DateTime::<Utc>::from);

        let artifact = match source {
            ArtifactSource::Trace => summarize_trace_file(&path, modified_at).await,
            ArtifactSource::Session => summarize_session_file(&path, modified_at).await,
            ArtifactSource::Legacy => summarize_legacy_file(&path, modified_at).await,
        };

        match artifact {
            Ok(artifact) => out.push(artifact),
            Err(err) => {
                warn!(error = %err, path = %path.display(), "failed to parse trajectory artifact")
            }
        }
    }

    Ok(out)
}

async fn summarize_legacy_file(
    path: &Path,
    modified_at: Option<DateTime<Utc>>,
) -> Result<TrajectoryArtifact> {
    Ok(TrajectoryArtifact {
        source: ArtifactSource::Legacy,
        short_label: short_label(path),
        artifact_path: path.to_path_buf(),
        title: format!("legacy trajectory {}", short_label(path)),
        status: TrajectoryStatus::Unknown,
        started_at: modified_at,
        updated_at: modified_at,
        task_description: None,
        summary: None,
        modified_files: Vec::new(),
        created_files: Vec::new(),
        target_paths: Vec::new(),
        target_modules: Vec::new(),
        total_tokens_used: None,
        task_count: 0,
        evidence: vec![crate::model::TrajectoryEvidence {
            label: "legacy artifact".to_string(),
            detail: "unstructured legacy trajectory snapshot".to_string(),
        }],
    })
}

fn short_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("trajectory")
        .to_string()
}

fn select_artifact(
    artifacts: &[TrajectoryArtifact],
    selector: Option<&str>,
) -> Option<TrajectoryArtifact> {
    match selector {
        None => artifacts.first().cloned(),
        Some(selector) => artifacts
            .iter()
            .find(|artifact| matches_selector(artifact, selector))
            .cloned(),
    }
}

fn path_matches(candidate: &str, normalized_query: &str) -> bool {
    let candidate = normalize_path_string(candidate);
    candidate == normalized_query
        || candidate.ends_with(&format!("/{normalized_query}"))
        || normalized_query.ends_with(&format!("/{candidate}"))
        || Path::new(&candidate).file_name().and_then(|s| s.to_str())
            == Path::new(normalized_query)
                .file_name()
                .and_then(|s| s.to_str())
}

fn normalize_query_path(root: &Path, file: &Path) -> String {
    let relative = if file.is_absolute() {
        file.strip_prefix(root).unwrap_or(file)
    } else {
        file
    };

    normalize_path_string(&relative.to_string_lossy())
}

fn normalize_path_string(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn normalized_path_to_module(path: &str) -> String {
    let mut normalized = path.trim_start_matches("./").replace('/', "::");
    if let Some(stripped) = normalized.strip_suffix(".rs") {
        normalized = stripped.to_string();
    }
    normalized
}

fn matches_selector(artifact: &TrajectoryArtifact, selector: &str) -> bool {
    artifact.short_label == selector
        || artifact.display_name() == selector
        || artifact.artifact_path.to_string_lossy() == selector
}

fn epoch() -> DateTime<Utc> {
    Utc.timestamp_opt(0, 0).single().unwrap_or_else(Utc::now)
}

fn match_priority(reason: FileMatchReason) -> u8 {
    match reason {
        FileMatchReason::ModifiedFile => 0,
        FileMatchReason::CreatedFile => 1,
        FileMatchReason::TargetPath => 2,
        FileMatchReason::TargetModule => 3,
        FileMatchReason::TraceArgument => 4,
    }
}
