//! `simplify` — collapse overlong text into a bounded summary.
//!
//! Intended for shrinking a large transcript, tool output, or file
//! excerpt down to a token-budget-friendly form. The implementation
//! is intentionally deterministic (no LLM): takes the first + last
//! portions and a line count. Drivers that want LLM-based summarisation
//! can ship a replacement skill under a different name.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

pub const DEFAULT_MAX_CHARS: usize = 2_000;
pub const MIN_MAX_CHARS: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SimplifyInput {
    text: String,
    #[serde(default)]
    max_chars: Option<usize>,
}

/// Build the collapsed form: keep the first half + last half of the
/// budget with an "... (N chars elided) ..." marker in the middle.
pub(crate) fn collapse(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    // Leave room for the marker.
    let marker_reserve = 64;
    let keep = max_chars.saturating_sub(marker_reserve).max(2);
    let half = keep / 2;
    let head: String = text.chars().take(half).collect();
    let total_chars = text.chars().count();
    let tail_start = total_chars - half;
    let tail: String = text.chars().skip(tail_start).collect();
    let elided = total_chars - (head.chars().count() + tail.chars().count());
    format!("{head}\n... ({elided} chars elided) ...\n{tail}")
}

pub struct SimplifySkill;

#[async_trait]
impl Skill for SimplifySkill {
    fn name(&self) -> &str {
        "simplify"
    }
    fn description(&self) -> &str {
        "Deterministically collapse overlong text to a bounded head-and-tail form."
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": { "type": "string" },
                "max_chars": { "type": "integer", "minimum": MIN_MAX_CHARS }
            }
        })
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome> {
        let parsed: SimplifyInput = serde_json::from_value(ctx.input.clone())
            .map_err(|e| SkillError::InvalidInput(e.to_string()))?;
        let cap = parsed.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
        if cap < MIN_MAX_CHARS {
            return Err(SkillError::InvalidInput(format!(
                "max_chars must be >= {MIN_MAX_CHARS}"
            )));
        }
        let original_chars = parsed.text.chars().count();
        let collapsed = collapse(&parsed.text, cap);
        let truncated = collapsed.chars().count() < original_chars;
        Ok(SkillOutcome::new(
            format!(
                "simplify: {} -> {} chars ({})",
                original_chars,
                collapsed.chars().count(),
                if truncated { "collapsed" } else { "unchanged" }
            ),
            json!({
                "collapsed": collapsed,
                "original_chars": original_chars,
                "truncated": truncated,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx(input: serde_json::Value) -> SkillContext {
        SkillContext::new(input, PathBuf::from("."))
    }

    #[test]
    fn collapse_noop_when_under_cap() {
        assert_eq!(collapse("short", 256), "short");
    }

    #[test]
    fn collapse_preserves_head_and_tail() {
        let text: String = "a".repeat(5_000);
        let c = collapse(&text, 512);
        assert!(c.contains("chars elided"));
        assert!(c.len() < 5_000);
    }

    #[tokio::test]
    async fn simplify_collapses_oversize_input() {
        let text: String = "x".repeat(10_000);
        let out = SimplifySkill
            .run(ctx(json!({ "text": text, "max_chars": 512 })))
            .await
            .unwrap();
        assert_eq!(out.payload["truncated"], true);
        assert!(out.payload["collapsed"].as_str().unwrap().contains("elided"));
    }

    #[tokio::test]
    async fn simplify_passes_through_short_input() {
        let out = SimplifySkill
            .run(ctx(json!({ "text": "hi" })))
            .await
            .unwrap();
        assert_eq!(out.payload["truncated"], false);
        assert_eq!(out.payload["collapsed"], "hi");
    }

    #[tokio::test]
    async fn simplify_rejects_too_small_budget() {
        let err = SimplifySkill
            .run(ctx(json!({ "text": "x", "max_chars": 100 })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn simplify_handles_multibyte_chars_safely() {
        let text: String = "日".repeat(2_000);
        let out = SimplifySkill
            .run(ctx(json!({ "text": text, "max_chars": 512 })))
            .await
            .unwrap();
        // No panic and payload is valid JSON — char-based slicing
        // preserves UTF-8 boundaries.
        assert_eq!(out.payload["truncated"], true);
    }
}
