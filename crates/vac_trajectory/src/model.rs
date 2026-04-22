use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactSource {
    Trace,
    Session,
    Legacy,
}

impl ArtifactSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Session => "session",
            Self::Legacy => "legacy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrajectoryStatus {
    Completed,
    Failed,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryEvidence {
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryArtifact {
    pub source: ArtifactSource,
    pub short_label: String,
    pub artifact_path: PathBuf,
    pub title: String,
    pub status: TrajectoryStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub task_description: Option<String>,
    pub summary: Option<String>,
    pub modified_files: Vec<String>,
    pub created_files: Vec<String>,
    pub target_paths: Vec<String>,
    pub target_modules: Vec<String>,
    pub total_tokens_used: Option<u64>,
    pub task_count: usize,
    pub evidence: Vec<TrajectoryEvidence>,
}

impl TrajectoryArtifact {
    pub fn display_name(&self) -> String {
        format!("{}: {}", self.source.as_str(), self.short_label)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainReport {
    pub artifact: TrajectoryArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileMatchReason {
    ModifiedFile,
    CreatedFile,
    TargetPath,
    TargetModule,
    TraceArgument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMatch {
    pub match_reason: FileMatchReason,
    pub detail: String,
    pub artifact: TrajectoryArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWhyReport {
    pub path: String,
    pub normalized_path: String,
    pub matches: Vec<FileMatch>,
}
