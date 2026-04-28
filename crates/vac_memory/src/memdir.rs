//! Memdir primitives — the shape each memory file takes on disk.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MemoryError, MemoryResult};

/// Which of the three shelves a memory lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryKind {
    /// Current applicable knowledge.
    Active,
    /// Superseded or stale — kept for audit, excluded from retrieval
    /// by default.
    Archived,
    /// Team-shared, committed to the repo.
    Team,
}

impl MemoryKind {
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Team => "team",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Active, Self::Archived, Self::Team]
    }
}

/// YAML frontmatter at the top of a memory file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    /// Short topic slug; filename stem.
    pub topic: String,
    /// Human-readable title (defaults to topic).
    #[serde(default)]
    pub title: Option<String>,
    /// Free-form tags — included in tf-idf corpus.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation timestamp; used by recency scoring.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last-updated timestamp; wins over `created_at` for recency.
    #[serde(default)]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Operator-specified relevance hint in [0.0, 1.0]; blended with
    /// scored relevance. `0.5` default = neutral.
    #[serde(default = "default_importance")]
    pub importance: f32,
    /// Policy name that produced this memory (if consolidator-written).
    #[serde(default)]
    pub source_policy: Option<String>,
}

fn default_importance() -> f32 {
    0.5
}

/// One memory file: frontmatter + markdown body + provenance.
#[derive(Debug, Clone)]
pub struct Memory {
    pub kind: MemoryKind,
    pub path: PathBuf,
    pub frontmatter: MemoryFrontmatter,
    pub body: String,
}

impl Memory {
    /// Return the timestamp used for recency scoring
    /// (`updated_at ?? created_at`).
    pub fn effective_timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        self.frontmatter
            .updated_at
            .unwrap_or(self.frontmatter.created_at)
    }

    /// Assemble the searchable text for retrieval: title + tags + body.
    pub fn corpus_text(&self) -> String {
        let mut s = String::new();
        if let Some(t) = &self.frontmatter.title {
            s.push_str(t);
            s.push('\n');
        }
        s.push_str(&self.frontmatter.topic);
        s.push('\n');
        for tag in &self.frontmatter.tags {
            s.push_str(tag);
            s.push('\n');
        }
        s.push_str(&self.body);
        s
    }
}

/// Parse a memory file's raw contents into frontmatter + body.
/// Expects the standard `---\n<yaml>\n---\n<body>` layout.
///
/// The closing delimiter must be an isolated `---` on its own line —
/// i.e. preceded by `\n` and followed by `\n` or EOF. This prevents a
/// markdown horizontal rule inside `body` from being mis-parsed as
/// the frontmatter terminator.
pub fn parse(content: &str) -> MemoryResult<(MemoryFrontmatter, String)> {
    let s = content
        .strip_prefix("---\n")
        .ok_or_else(|| MemoryError::Frontmatter("missing opening '---' delimiter".into()))?;
    // Scan for a line that is exactly "---" (no leading/trailing space).
    let mut offset = 0usize;
    let mut closing_line_start: Option<usize> = None;
    let mut closing_line_end: Option<usize> = None;
    for line in s.split_inclusive('\n') {
        let line_len = line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            closing_line_start = Some(offset);
            closing_line_end = Some(offset + line_len);
            break;
        }
        offset += line_len;
    }
    // Last-line-without-trailing-newline fallback (`---` at EOF).
    if closing_line_start.is_none() && s.ends_with("\n---") {
        let start = s.len() - 3;
        closing_line_start = Some(start);
        closing_line_end = Some(s.len());
    }
    let close_start = closing_line_start
        .ok_or_else(|| MemoryError::Frontmatter("missing closing '---' delimiter".into()))?;
    let close_end = closing_line_end.unwrap_or(s.len());
    // YAML is everything up to (but not including) the closing line.
    // Strip the trailing `\n` that belongs to the line before `---`.
    let yaml = s[..close_start].trim_end_matches('\n');
    let body_raw = &s[close_end..];
    let body = body_raw.trim_start_matches('\n').to_string();
    let fm: MemoryFrontmatter = serde_yaml::from_str(yaml)?;
    Ok((fm, body))
}

/// Serialize frontmatter + body back to the memdir file format.
pub fn serialize(fm: &MemoryFrontmatter, body: &str) -> MemoryResult<String> {
    let yaml = serde_yaml::to_string(fm)?;
    Ok(format!("---\n{yaml}---\n{body}"))
}

/// Produce the expected path for a topic on a given shelf under `root`.
pub fn path_for(root: &Path, kind: MemoryKind, topic: &str) -> PathBuf {
    root.join(kind.dir_name()).join(format!("{topic}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_memdir_file() {
        let fm = MemoryFrontmatter {
            topic: "build-discipline".into(),
            title: Some("Build discipline".into()),
            tags: vec!["rust".into(), "cargo".into()],
            created_at: chrono::Utc::now(),
            updated_at: None,
            importance: 0.7,
            source_policy: Some("workflow_learnings".into()),
        };
        let body = "cargo test is blocked; use nextest.\n";
        let raw = serialize(&fm, body).unwrap();
        let (fm_back, body_back) = parse(&raw).unwrap();
        assert_eq!(fm_back.topic, fm.topic);
        assert_eq!(fm_back.tags, fm.tags);
        assert_eq!(body_back, body);
    }

    #[test]
    fn parse_rejects_missing_frontmatter() {
        let r = parse("hello no frontmatter");
        assert!(r.is_err());
    }

    #[test]
    fn body_with_markdown_hr_survives_roundtrip() {
        // A markdown horizontal rule inside the body must not be
        // mistaken for the frontmatter terminator.
        let fm = MemoryFrontmatter {
            topic: "hr-body".into(),
            title: None,
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: None,
            importance: 0.5,
            source_policy: None,
        };
        let body = "before\n\n---\n\nafter the hr\n";
        let raw = serialize(&fm, body).unwrap();
        let (fm_back, body_back) = parse(&raw).unwrap();
        assert_eq!(fm_back.topic, "hr-body");
        assert!(body_back.contains("before"));
        assert!(body_back.contains("after the hr"));
        assert!(body_back.contains("---"));
    }

    #[test]
    fn parse_handles_unicode_in_frontmatter_and_body() {
        let fm = MemoryFrontmatter {
            topic: "unicode".into(),
            title: Some("プロファイル drift — 日本語".into()),
            tags: vec!["日本語".into(), "émoji-✓".into()],
            created_at: chrono::Utc::now(),
            updated_at: None,
            importance: 0.5,
            source_policy: None,
        };
        let body = "日本語ボディー with — em-dash\n";
        let raw = serialize(&fm, body).unwrap();
        let (fm_back, body_back) = parse(&raw).unwrap();
        assert_eq!(fm_back.tags, fm.tags);
        assert_eq!(body_back, body);
    }

    #[test]
    fn effective_timestamp_prefers_updated_at() {
        let created = chrono::Utc::now() - chrono::Duration::days(30);
        let updated = chrono::Utc::now();
        let m = Memory {
            kind: MemoryKind::Active,
            path: PathBuf::from("x.md"),
            frontmatter: MemoryFrontmatter {
                topic: "t".into(),
                title: None,
                tags: vec![],
                created_at: created,
                updated_at: Some(updated),
                importance: 0.5,
                source_policy: None,
            },
            body: String::new(),
        };
        assert_eq!(m.effective_timestamp(), updated);
    }
}
