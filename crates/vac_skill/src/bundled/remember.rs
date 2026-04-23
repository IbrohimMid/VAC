//! `remember` — write an operator-invoked memory entry to disk.
//!
//! Persists under `<working_dir>/.vac/skill-memory/<topic>.md` with
//! a front-matter line that carries the invocation timestamp. Uses
//! `tokio::fs` throughout — no sync I/O on the async path.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RememberInput {
    /// Slug-ish topic; the skill sanitizes to `[a-z0-9-]+` when
    /// constructing the filename so a user-supplied value can't
    /// escape the memory dir via path traversal.
    topic: String,
    content: String,
}

/// Visible for tests — slug rules are security-relevant, so they
/// live as a named function we can audit.
pub(crate) fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == '-' || c == '_' || c == ' ' {
            out.push('-');
        }
        // everything else (including `/`, `..`, null) is dropped.
    }
    // Collapse runs of `-` and trim ends.
    let collapsed: String = out
        .split('-')
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    collapsed
}

pub struct RememberSkill;

#[async_trait]
impl Skill for RememberSkill {
    fn name(&self) -> &str {
        "remember"
    }
    fn description(&self) -> &str {
        "Persist a named memory entry under .vac/skill-memory/. Topic is slugified for safety."
    }
    // Remember mutates the filesystem; keep default is_read_only=false.
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["topic", "content"],
            "properties": {
                "topic": { "type": "string", "minLength": 1 },
                "content": { "type": "string", "minLength": 1 }
            }
        })
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome> {
        let parsed: RememberInput = serde_json::from_value(ctx.input.clone())
            .map_err(|e| SkillError::InvalidInput(e.to_string()))?;
        let topic = slug(&parsed.topic);
        if topic.is_empty() {
            return Err(SkillError::InvalidInput(
                "topic must contain at least one alphanumeric character".into(),
            ));
        }
        let dir = ctx.working_dir.join(".vac").join("skill-memory");
        tokio::fs::create_dir_all(&dir).await?;
        // Refuse to write through a symlink — a compromised or
        // misconfigured .vac/skill-memory could point outside the
        // project tree. `symlink_metadata` does not follow links.
        let lmeta = tokio::fs::symlink_metadata(&dir).await?;
        if lmeta.file_type().is_symlink() {
            return Err(SkillError::Execution(format!(
                "refusing to write through symlink: {}",
                dir.display()
            )));
        }
        let path = dir.join(format!("{topic}.md"));
        let body = format!(
            "---\ntopic: {topic}\nwritten_at: {ts}\n---\n\n{content}\n",
            topic = topic,
            ts = chrono_ts(),
            content = parsed.content,
        );
        tokio::fs::write(&path, body).await?;
        Ok(SkillOutcome::new(
            format!("remembered: {topic}"),
            json!({
                "topic": topic,
                "path": path,
                "bytes": parsed.content.len(),
            }),
        ))
    }
}

fn chrono_ts() -> String {
    // Keep formatting pure-std so the crate avoids an extra dep.
    // UNIX seconds are enough granularity — memory entries usually
    // live in the minutes-to-days time-scale.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx_for(tmp: &tempfile::TempDir, input: serde_json::Value) -> SkillContext {
        SkillContext::new(input, tmp.path().to_path_buf())
    }

    #[test]
    fn slug_keeps_alphanumeric_and_collapses_separators() {
        assert_eq!(slug("Hello World"), "hello-world");
        assert_eq!(slug("a//b"), "ab");
        assert_eq!(slug("x--y"), "x-y");
        assert_eq!(slug("A B_C"), "a-b-c");
        assert_eq!(slug("...leading.dots"), "leadingdots");
    }

    #[test]
    fn slug_rejects_path_traversal() {
        assert_eq!(slug("../../etc/passwd"), "etcpasswd");
        // Resulting slug is harmless — no `/` or `..` survive.
    }

    #[test]
    fn slug_empty_on_punctuation_only() {
        assert_eq!(slug("...!!!"), "");
    }

    #[tokio::test]
    async fn remember_writes_file_under_memory_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let out = RememberSkill
            .run(ctx_for(
                &tmp,
                json!({
                    "topic": "Session learnings",
                    "content": "prefer nextest over test"
                }),
            ))
            .await
            .unwrap();
        let path: PathBuf = serde_json::from_value(out.payload["path"].clone()).unwrap();
        assert!(path.ends_with(".vac/skill-memory/session-learnings.md"));
        let body = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(body.contains("prefer nextest"));
        assert!(body.contains("topic: session-learnings"));
    }

    #[tokio::test]
    async fn remember_with_unslug_friendly_topic_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let err = RememberSkill
            .run(ctx_for(&tmp, json!({ "topic": "!!!", "content": "x" })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn remember_refuses_to_write_through_symlink() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        // Create a sibling dir and symlink .vac/skill-memory to it.
        // The symlink points within the tempdir so it's a harmless
        // probe — the point is the guard fires regardless of target.
        let outside = tempfile::tempdir().unwrap();
        let vac_dir = tmp.path().join(".vac");
        tokio::fs::create_dir_all(&vac_dir).await.unwrap();
        symlink(outside.path(), vac_dir.join("skill-memory")).unwrap();
        let err = RememberSkill
            .run(ctx_for(&tmp, json!({ "topic": "x", "content": "y" })))
            .await
            .unwrap_err();
        match err {
            SkillError::Execution(msg) => assert!(msg.contains("symlink")),
            other => panic!("expected Execution error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn remember_overwrites_same_topic() {
        let tmp = tempfile::tempdir().unwrap();
        RememberSkill
            .run(ctx_for(&tmp, json!({ "topic": "x", "content": "first" })))
            .await
            .unwrap();
        RememberSkill
            .run(ctx_for(&tmp, json!({ "topic": "x", "content": "second" })))
            .await
            .unwrap();
        let path = tmp.path().join(".vac/skill-memory/x.md");
        let body = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(body.contains("second"));
        assert!(!body.contains("first"), "latest write wins");
    }
}
