//! B.3 — markdown-backed skills registry.
//!
//! Each file under `<project_root>/.vac/skills/*.md` defines one
//! skill. The file format is:
//!
//! ```markdown
//! ---
//! name: simplify
//! description: Run the simplify pass on the current changes.
//! triggers:
//!   - "refactor"
//!   - "dead code"
//! tools:
//!   - Grep
//!   - Edit
//! ---
//! Body of the skill (used as the system prompt for a skill-
//! invocation subagent). Plain markdown from this point down.
//! ```
//!
//! The loader is pure — no network, no tool registry dep. Hosts
//! decide whether to register each `MarkdownSkill` into
//! `SkillRegistry` or dispatch via the Agent tool as a
//! `Custom(name)` subagent.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{SkillError, SkillResult};

/// Parsed markdown skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarkdownSkill {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Regex patterns; the hook layer can watch user messages and
    /// suggest this skill when one matches.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Optional allowlist of tools the skill may invoke. When
    /// empty, the default CompositeGate rules apply.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Prompt body — everything after the YAML frontmatter block.
    #[serde(default)]
    pub prompt: String,
    /// Source file, for operator-facing "where did this come from"
    /// diagnostics.
    #[serde(skip, default)]
    pub source: Option<PathBuf>,
}

impl MarkdownSkill {
    /// Parse a single markdown file's contents.
    pub fn parse(raw: &str) -> SkillResult<Self> {
        parse_markdown(raw, None)
    }

    /// Parse from disk. Convenience wrapper capturing the source
    /// path for diagnostics.
    pub async fn load(path: &Path) -> SkillResult<Self> {
        let raw = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| SkillError::Other(anyhow::anyhow!("read {}: {e}", path.display())))?;
        parse_markdown(&raw, Some(path.to_path_buf()))
    }
}

fn parse_markdown(raw: &str, source: Option<PathBuf>) -> SkillResult<MarkdownSkill> {
    // Expect frontmatter delimited by `---` lines. Missing =>
    // treat the full file as prompt with only the filename as
    // identity (callers supply `name` via the path in that case).
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let fm = &rest[..end];
            let body = &rest[end + 5..];
            // Minimal YAML subset: `key: value` or `key:` followed
            // by `- item` lines. Full yaml parsing is a dep we
            // avoid in this crate; upstream drivers can swap in
            // `serde_yaml` later without changing this API.
            let parsed = parse_front_matter(fm);
            let mut skill = MarkdownSkill {
                name: parsed.name,
                description: parsed.description,
                triggers: parsed.triggers,
                tools: parsed.tools,
                prompt: body.trim_start_matches('\n').to_string(),
                source: source.clone(),
            };
            if skill.name.is_empty() {
                if let Some(src) = source.as_ref() {
                    skill.name = src
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unnamed")
                        .to_string();
                }
            }
            if skill.name.is_empty() {
                return Err(SkillError::InvalidInput(
                    "skill frontmatter missing `name`".into(),
                ));
            }
            return Ok(skill);
        }
    }
    // No frontmatter: derive name from source filename.
    let name = source
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_string();
    if name == "unnamed" {
        return Err(SkillError::InvalidInput(
            "skill file has no frontmatter and no filename to derive a name".into(),
        ));
    }
    Ok(MarkdownSkill {
        name,
        description: String::new(),
        triggers: Vec::new(),
        tools: Vec::new(),
        prompt: trimmed.to_string(),
        source,
    })
}

#[derive(Debug, Default)]
struct FrontMatter {
    name: String,
    description: String,
    triggers: Vec<String>,
    tools: Vec<String>,
}

fn parse_front_matter(fm: &str) -> FrontMatter {
    let mut out = FrontMatter::default();
    let mut current_list: Option<&'static str> = None;
    for line in fm.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(item) = trimmed.trim_start().strip_prefix("- ") {
            let value = item.trim().trim_matches('"').trim_matches('\'').to_string();
            match current_list {
                Some("triggers") => out.triggers.push(value),
                Some("tools") => out.tools.push(value),
                _ => {}
            }
            continue;
        }
        current_list = None;
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if value.is_empty() {
                match key {
                    "triggers" => current_list = Some("triggers"),
                    "tools" => current_list = Some("tools"),
                    _ => {}
                }
            } else {
                let clean = value.trim_matches('"').trim_matches('\'').to_string();
                match key {
                    "name" => out.name = clean,
                    "description" => out.description = clean,
                    _ => {}
                }
            }
        }
    }
    out
}

/// Load every `.md` under `dir`. Missing dir is not an error —
/// returns an empty vec so fresh projects don't explode.
pub async fn load_skills_from_dir(dir: &Path) -> SkillResult<Vec<MarkdownSkill>> {
    let mut out = Vec::new();
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(out);
    }
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| SkillError::Other(anyhow::anyhow!("read_dir {}: {e}", dir.display())))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| SkillError::Other(anyhow::anyhow!(e.to_string())))?
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        match MarkdownSkill::load(&path).await {
            Ok(skill) => out.push(skill),
            Err(e) => {
                tracing::warn!(
                    target: "vac_skill::md_registry",
                    path = %path.display(),
                    error = %e,
                    "skill load failed — skipping",
                );
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---
name: simplify
description: Run the simplify pass.
triggers:
  - refactor
  - dead code
tools:
  - Grep
  - Edit
---
Body line one.
Body line two.
";

    #[test]
    fn parse_roundtrips_a_well_formed_skill() {
        let s = MarkdownSkill::parse(SAMPLE).unwrap();
        assert_eq!(s.name, "simplify");
        assert_eq!(s.description, "Run the simplify pass.");
        assert_eq!(s.triggers, vec!["refactor", "dead code"]);
        assert_eq!(s.tools, vec!["Grep", "Edit"]);
        assert!(s.prompt.starts_with("Body line one"));
    }

    #[tokio::test]
    async fn load_from_dir_picks_up_only_md() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("simplify.md"), SAMPLE)
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("ignored.txt"), "nope")
            .await
            .unwrap();
        let list = load_skills_from_dir(tmp.path()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "simplify");
    }

    #[tokio::test]
    async fn missing_dir_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let list = load_skills_from_dir(&missing).await.unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn empty_frontmatter_requires_filename() {
        // Parse directly (no filename): should error.
        let err = MarkdownSkill::parse("plain body, no frontmatter").unwrap_err();
        assert!(format!("{err}").contains("no frontmatter"));
    }
}
