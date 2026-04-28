//! `verify` — read-only self-audit of the last mutation.
//!
//! Given a list of files the agent claims to have changed, re-checks
//! that each one still exists and that at least one of the expected
//! markers (substring match) appears in the file. Emits a
//! `{ ok, issues }` verdict the driver can surface directly.
//!
//! Deliberately does not invoke the LLM or run tools — `verify` is
//! the shortest-path reality check before a commit.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

/// Read-size cap per file. Verify's job is "does this file contain
/// the marker" — a few KB is plenty; a huge file is almost certainly
/// a misuse or a denial-of-service probe.
pub const MAX_VERIFY_READ_BYTES: u64 = 1_048_576; // 1 MiB

/// Resolve `rel` (absolute or relative) against `working_dir`
/// canonically and confirm the result stays inside `working_dir`.
/// Used to reject path traversal — `../../etc/passwd`, symlinks
/// pointing outside the project, absolute paths to arbitrary
/// files — regardless of input shape.
pub(crate) async fn resolve_within(working_dir: &Path, rel: &Path) -> Result<PathBuf, String> {
    let base = tokio::fs::canonicalize(working_dir)
        .await
        .map_err(|e| format!("working_dir unreadable: {e}"))?;
    let joined = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        working_dir.join(rel)
    };
    let real = tokio::fs::canonicalize(&joined)
        .await
        .map_err(|e| format!("path resolve failed: {e}"))?;
    if !real.starts_with(&base) {
        return Err(format!("path escapes working_dir: {}", real.display()));
    }
    Ok(real)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifyInput {
    #[serde(default)]
    changed_paths: Vec<PathBuf>,
    /// Optional markers each file should contain. When empty, the
    /// skill checks only that the file exists and is non-empty.
    #[serde(default)]
    expected_markers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifyIssue {
    path: PathBuf,
    reason: String,
}

pub struct VerifySkill;

#[async_trait]
impl Skill for VerifySkill {
    fn name(&self) -> &str {
        "verify"
    }
    fn description(&self) -> &str {
        "Self-audit the agent's last mutation: each named file exists, is non-empty, and contains the expected markers."
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "changed_paths": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "expected_markers": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        })
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome> {
        let parsed: VerifyInput = serde_json::from_value(ctx.input.clone())
            .map_err(|e| SkillError::InvalidInput(e.to_string()))?;
        if parsed.changed_paths.is_empty() {
            return Err(SkillError::InvalidInput(
                "verify requires at least one changed_paths entry".into(),
            ));
        }
        let mut issues: Vec<VerifyIssue> = Vec::new();
        for rel in &parsed.changed_paths {
            // Reject any path that escapes the project tree BEFORE
            // we read metadata — even an existence check against
            // /etc/passwd is a leak. `resolve_within` canonicalises
            // so symlinks pointing outside the project are rejected.
            let abs = match resolve_within(&ctx.working_dir, rel).await {
                Ok(p) => p,
                Err(reason) => {
                    issues.push(VerifyIssue {
                        path: rel.clone(),
                        reason,
                    });
                    continue;
                }
            };
            let meta = match tokio::fs::metadata(&abs).await {
                Ok(m) => m,
                Err(_) => {
                    issues.push(VerifyIssue {
                        path: abs.clone(),
                        reason: "path does not exist".into(),
                    });
                    continue;
                }
            };
            if meta.len() == 0 {
                issues.push(VerifyIssue {
                    path: abs.clone(),
                    reason: "file is empty".into(),
                });
                continue;
            }
            if meta.len() > MAX_VERIFY_READ_BYTES {
                issues.push(VerifyIssue {
                    path: abs.clone(),
                    reason: format!(
                        "file exceeds verify read cap ({} > {MAX_VERIFY_READ_BYTES} bytes)",
                        meta.len()
                    ),
                });
                continue;
            }
            if !parsed.expected_markers.is_empty() {
                let content = tokio::fs::read_to_string(&abs).await?;
                for marker in &parsed.expected_markers {
                    if !content.contains(marker) {
                        issues.push(VerifyIssue {
                            path: abs.clone(),
                            reason: format!("missing marker: {marker}"),
                        });
                    }
                }
            }
        }
        let ok = issues.is_empty();
        let summary = if ok {
            format!("verify: {} file(s) ok", parsed.changed_paths.len())
        } else {
            format!("verify: {} issue(s)", issues.len())
        };
        Ok(SkillOutcome::new(
            summary,
            json!({
                "ok": ok,
                "issues": issues,
                "checked": parsed.changed_paths.len(),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(tmp: &tempfile::TempDir, input: serde_json::Value) -> SkillContext {
        SkillContext::new(input, tmp.path().to_path_buf())
    }

    #[tokio::test]
    async fn verify_empty_paths_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let err = VerifySkill
            .run(ctx(&tmp, json!({ "changed_paths": [] })))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn verify_ok_when_file_exists_and_has_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.rs");
        tokio::fs::write(&p, "fn hello() {}\n").await.unwrap();
        let out = VerifySkill
            .run(ctx(
                &tmp,
                json!({
                    "changed_paths": ["a.rs"],
                    "expected_markers": ["hello"]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], true);
    }

    #[tokio::test]
    async fn verify_flags_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let out = VerifySkill
            .run(ctx(&tmp, json!({ "changed_paths": ["nope.rs"] })))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], false);
        let issues = out.payload["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 1);
        // After W3 audit the path resolver surfaces the canonicalize
        // error on missing files ("path resolve failed: …"). Accept
        // either phrasing so the test stays robust across resolver
        // revisions.
        let reason = issues[0]["reason"].as_str().unwrap();
        assert!(
            reason.contains("does not exist") || reason.contains("path resolve failed"),
            "unexpected reason: {reason}",
        );
    }

    #[tokio::test]
    async fn verify_flags_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("empty.txt");
        tokio::fs::write(&p, "").await.unwrap();
        let out = VerifySkill
            .run(ctx(&tmp, json!({ "changed_paths": ["empty.txt"] })))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], false);
        assert!(
            out.payload["issues"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("empty")
        );
    }

    #[tokio::test]
    async fn verify_flags_missing_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.rs");
        tokio::fs::write(&p, "fn foo() {}\n").await.unwrap();
        let out = VerifySkill
            .run(ctx(
                &tmp,
                json!({
                    "changed_paths": ["a.rs"],
                    "expected_markers": ["bar"]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], false);
        let reason = out.payload["issues"][0]["reason"].as_str().unwrap();
        assert!(reason.contains("missing marker: bar"));
    }

    #[tokio::test]
    async fn verify_accepts_absolute_paths_inside_project() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.rs");
        tokio::fs::write(&p, "x\n").await.unwrap();
        let out = VerifySkill
            .run(ctx(&tmp, json!({ "changed_paths": [p.to_str().unwrap()] })))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], true);
    }

    #[tokio::test]
    async fn verify_rejects_absolute_path_outside_project() {
        let tmp = tempfile::tempdir().unwrap();
        // `/etc/hostname` exists on every linux box and is tiny.
        // If the skill honours containment, it will flag it as an
        // issue rather than happily confirming its contents.
        let out = VerifySkill
            .run(ctx(&tmp, json!({ "changed_paths": ["/etc/hostname"] })))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], false);
        let reason = out.payload["issues"][0]["reason"].as_str().unwrap();
        assert!(
            reason.contains("escapes working_dir"),
            "expected containment error, got: {reason}",
        );
    }

    #[tokio::test]
    async fn verify_rejects_dotdot_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let out = VerifySkill
            .run(ctx(
                &tmp,
                json!({ "changed_paths": ["../../etc/hostname"] }),
            ))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], false);
    }

    #[tokio::test]
    async fn verify_flags_file_over_read_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("big.bin");
        // Fill with MAX+1 bytes. Using write_all with a vec of zeros
        // is O(N) but a megabyte is fine for a unit test.
        let data = vec![b'x'; (MAX_VERIFY_READ_BYTES as usize) + 1];
        tokio::fs::write(&p, &data).await.unwrap();
        let out = VerifySkill
            .run(ctx(
                &tmp,
                json!({
                    "changed_paths": ["big.bin"],
                    "expected_markers": ["x"]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(out.payload["ok"], false);
        let reason = out.payload["issues"][0]["reason"].as_str().unwrap();
        assert!(reason.contains("exceeds verify read cap"));
    }
}
