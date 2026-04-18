//! Plan mode utilities: front matter parsing, metadata, and file I/O.
//!
//! The plan is stored at `<project_root>/.vac/session/plan.md` with YAML
//! front matter containing metadata (title, status, version, timestamps).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

pub const PLAN_FILENAME: &str = "plan.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    #[default]
    #[serde(alias = "draft")]
    Drafting,
    #[serde(alias = "reviewing")]
    PendingReview,
    #[serde(alias = "approved")]
    Approved,
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanStatus::Drafting => write!(f, "drafting"),
            PlanStatus::PendingReview => write!(f, "pending_review"),
            PlanStatus::Approved => write!(f, "approved"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMetadata {
    pub title: String,
    pub status: PlanStatus,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub created: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated: Option<DateTime<Utc>>,
}

fn default_version() -> u32 {
    1
}

pub fn parse_plan_front_matter(content: &str) -> Option<PlanMetadata> {
    let trimmed = content.trim_start();
    let after_opening = trimmed.strip_prefix("---")?;
    let closing_pos = after_opening.find("\n---")?;
    let yaml_str = after_opening.get(..closing_pos)?.trim();
    if yaml_str.is_empty() {
        return None;
    }
    serde_yaml::from_str(yaml_str).ok()
}

pub fn extract_plan_body(content: &str) -> &str {
    let trimmed = content.trim_start();
    let Some(after_opening) = trimmed.strip_prefix("---") else {
        return content;
    };
    if let Some(closing_pos) = after_opening.find("\n---") {
        let rest_start = closing_pos + "\n---".len();
        let after_closing = after_opening.get(rest_start..).unwrap_or("");
        after_closing.strip_prefix('\n').unwrap_or(after_closing)
    } else {
        content
    }
}

pub fn session_dir(project_root: &Path) -> PathBuf {
    project_root.join(".vac").join("session")
}

pub fn plan_file_path(project_root: &Path) -> PathBuf {
    session_dir(project_root).join(PLAN_FILENAME)
}

pub fn plan_file_exists(project_root: &Path) -> bool {
    plan_file_path(project_root).exists()
}

pub fn read_plan_file(project_root: &Path) -> Option<(PlanMetadata, String)> {
    let path = plan_file_path(project_root);
    let content = std::fs::read_to_string(path).ok()?;
    let metadata = parse_plan_front_matter(&content)?;
    Some((metadata, content))
}

/// Write plan content to disk, creating the session directory if needed.
pub fn write_plan_file(project_root: &Path, content: &str) -> std::io::Result<()> {
    let dir = session_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(plan_file_path(project_root), content)
}

pub fn compute_plan_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Build an initial plan template for a new plan.
pub fn new_plan_template(title: &str) -> String {
    let now = Utc::now().to_rfc3339();
    format!(
        "---\ntitle: {title}\nstatus: drafting\nversion: 1\ncreated: {now}\n---\n\n## Overview\n\n(describe the change)\n\n## Steps\n\n- [ ] first step\n- [ ] second step\n\n## Verification\n\n(how to confirm the change works)\n"
    )
}

/// Archive the existing plan by renaming it with its creation timestamp.
pub fn archive_plan_file(project_root: &Path) -> Option<PathBuf> {
    let plan_path = plan_file_path(project_root);
    if !plan_path.exists() {
        return None;
    }
    let ts = std::fs::read_to_string(&plan_path)
        .ok()
        .and_then(|c| parse_plan_front_matter(&c))
        .and_then(|m| m.created)
        .or_else(|| {
            std::fs::metadata(&plan_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from)
        })
        .unwrap_or_else(Utc::now)
        .format("%Y%m%d_%H%M%S");
    let archive_path = session_dir(project_root).join(format!("plan.{ts}.md"));
    std::fs::rename(&plan_path, &archive_path).ok()?;
    Some(archive_path)
}
