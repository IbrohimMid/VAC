//! vil_ir_diff — agent-callable tool that runs IR-level diff on a file.
//!
//! Wraps `vil_ir::diff::diff_modules` so the agent can invoke semantic IR
//! comparison as a real tool call instead of a canned prompt.
//!
//! Modes:
//! - **Worktree vs revision (default)**: `from_rev` (default `HEAD`) against
//!   the current on-disk file.
//! - **Rev-range**: supply both `from` and `to` — each side is loaded via
//!   `git show <rev>:<file>` and diffed; the worktree is not touched.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vil_ir::{FunctionRename, IrDiffReport};

#[derive(Debug, Deserialize)]
struct DiffInput {
    /// File path relative to working_dir.
    file: String,
    /// Legacy single-rev mode: compare this revision against the worktree.
    /// Ignored when both `from` and `to` are supplied.
    #[serde(default = "default_from_rev")]
    from_rev: String,
    /// Rev-range mode: source revision.
    #[serde(default)]
    from: Option<String>,
    /// Rev-range mode: target revision.
    #[serde(default)]
    to: Option<String>,
}

fn default_from_rev() -> String {
    "HEAD".to_string()
}

#[derive(Debug, Serialize)]
struct DiffOutput {
    file: String,
    mode: &'static str,
    overall_kind: String,
    modifies_generated_region: bool,
    changes: Vec<ChangeEntry>,
    renames: Vec<RenameEntry>,
    summary: String,
    summary_lines: Vec<String>,
    total_semantic: usize,
    total_cosmetic: usize,
}

#[derive(Debug, Serialize)]
struct ChangeEntry {
    entity_name: String,
    entity_type: String,
    kind: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct RenameEntry {
    module_path: String,
    old_name: String,
    new_name: String,
    signature_match_score: f32,
}

use crate::security::validate_path_within_root;

/// Validate that a git revision string contains only safe characters.
fn validate_rev(rev: &str) -> Result<(), ToolError> {
    if rev.is_empty() || rev.len() > 128 {
        return Err(ToolError::InvalidArguments(
            "Invalid revision: empty or too long".into(),
        ));
    }
    if !rev
        .chars()
        .all(|c| c.is_alphanumeric() || "~^./-@{}:_".contains(c))
    {
        return Err(ToolError::InvalidArguments(
            "Invalid revision format: contains disallowed characters".into(),
        ));
    }
    Ok(())
}

/// Load a file at a given git revision and parse it as an `IrModule`.
/// Returns `Ok(None)` when the path does not exist at that revision (e.g.
/// newly-added or deleted file), `Err` on other git/parse failures.
fn load_ir_at_rev(
    working_dir: &std::path::Path,
    rev: &str,
    safe_rel: &str,
) -> Result<Option<vil_ir::IrModule>, ToolError> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{rev}:{safe_rel}")])
        .current_dir(working_dir)
        .output()
        .map_err(|e| ToolError::ExecutionFailed(format!("git show failed: {e}")))?;

    if !output.status.success() {
        // File absent at this revision — caller treats this as "added" / "deleted".
        return Ok(None);
    }

    let content = String::from_utf8_lossy(&output.stdout);
    let module = vil_ir::parser::parse_source(&content, safe_rel)
        .map_err(|e| ToolError::ExecutionFailed(format!("parse @ {rev}: {e}")))?;
    Ok(Some(module))
}

pub struct VilIrDiffTool;

impl VilIrDiffTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilIrDiffTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for VilIrDiffTool {
    fn name(&self) -> &str {
        "vil_ir_diff"
    }

    fn description(&self) -> &str {
        "Compare the IR (functions, structs, VIL attributes) of a file between two \
        git revisions, or between a single revision and the worktree. Reports \
        semantic vs cosmetic changes, detects function renames, flags modifications \
        to VIL-generated regions, and emits agent-consumable summary lines."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "File path relative to project root"
                },
                "from_rev": {
                    "type": "string",
                    "description": "Legacy: single git revision to compare against the worktree (default: HEAD). Ignored when both `from` and `to` are provided."
                },
                "from": {
                    "type": "string",
                    "description": "Rev-range mode: source git revision. Requires `to`."
                },
                "to": {
                    "type": "string",
                    "description": "Rev-range mode: target git revision. Requires `from`."
                }
            },
            "required": ["file"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: DiffInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let abs_path = validate_path_within_root(&context.working_dir, &input.file)?;
        let safe_rel = abs_path
            .strip_prefix(&context.working_dir)
            .unwrap_or(&abs_path)
            .display()
            .to_string();

        // Resolve mode + validate all revs that will be used.
        let (old_module, new_module, mode): (_, _, &'static str) = match (&input.from, &input.to) {
            (Some(from), Some(to)) => {
                validate_rev(from)?;
                validate_rev(to)?;
                let old = load_ir_at_rev(&context.working_dir, from, &safe_rel)?;
                let new = load_ir_at_rev(&context.working_dir, to, &safe_rel)?;
                (old, new, "rev_range")
            }
            _ => {
                validate_rev(&input.from_rev)?;
                let old = load_ir_at_rev(&context.working_dir, &input.from_rev, &safe_rel)?;
                let new = Some(vil_ir::parser::parse_file(&abs_path).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to parse current file: {e}"))
                })?);
                (old, new, "rev_vs_worktree")
            }
        };

        let diff = vil_ir::diff::diff_modules(old_module.as_ref(), new_module.as_ref());

        let Some(d) = diff else {
            return Ok(serde_json::json!({
                "file": input.file,
                "mode": mode,
                "overall_kind": "None",
                "modifies_generated_region": false,
                "changes": [],
                "renames": [],
                "summary": "",
                "summary_lines": [],
                "total_semantic": 0,
                "total_cosmetic": 0,
                "note": "No IR differences detected"
            }));
        };

        let modifies_generated_region = d.modifies_generated_region;
        let overall_kind = match d.overall_kind {
            vil_ir::ChangeKind::Semantic => "Semantic".to_string(),
            vil_ir::ChangeKind::Cosmetic => "Cosmetic".to_string(),
        };

        let report = IrDiffReport::from_module_diffs(vec![d]);

        let changes: Vec<ChangeEntry> = report
            .module_diffs
            .iter()
            .flat_map(|md| md.changes.iter())
            .map(|c| ChangeEntry {
                entity_name: c.entity_name.clone(),
                entity_type: c.entity_type.clone(),
                kind: match c.kind {
                    vil_ir::ChangeKind::Semantic => "Semantic".to_string(),
                    vil_ir::ChangeKind::Cosmetic => "Cosmetic".to_string(),
                },
                description: c.description.clone(),
            })
            .collect();

        let renames: Vec<RenameEntry> = report
            .renames
            .iter()
            .map(|r: &FunctionRename| RenameEntry {
                module_path: r.module_path.clone(),
                old_name: r.old_name.clone(),
                new_name: r.new_name.clone(),
                signature_match_score: r.signature_match_score,
            })
            .collect();

        let summary = report.summary_lines.join("\n");

        let output = DiffOutput {
            file: input.file,
            mode,
            overall_kind,
            modifies_generated_region,
            changes,
            renames,
            summary,
            summary_lines: report.summary_lines,
            total_semantic: report.semantic_count,
            total_cosmetic: report.cosmetic_count,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ToolContext;
    use std::path::PathBuf;
    use std::process::Command;

    /// Run `git` in `dir`, panicking on failure (for test fixtures only).
    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git should exist on PATH for tests");
        assert!(status.success(), "git {args:?} failed in {dir:?}");
    }

    fn init_fixture_repo() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();

        // Use -c to avoid relying on ambient git identity config.
        let init = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&root)
            .status()
            .expect("git init");
        assert!(init.success());
        // Configure identity locally (required for commits).
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "user.name", "test"]);
        git(&root, &["config", "commit.gpgsign", "false"]);

        (tmp, root)
    }

    #[tokio::test]
    async fn rev_range_detects_rename_across_commits() {
        let (_tmp, root) = init_fixture_repo();

        let src = root.join("src.rs");
        std::fs::write(&src, "pub fn foo(x: i32) -> bool { x > 0 }\n").unwrap();
        git(&root, &["add", "src.rs"]);
        git(&root, &["commit", "-q", "-m", "v1"]);

        std::fs::write(&src, "pub fn bar(x: i32) -> bool { x > 0 }\n").unwrap();
        git(&root, &["add", "src.rs"]);
        git(&root, &["commit", "-q", "-m", "v2"]);

        // Canonical working_dir so validate_path_within_root round-trips cleanly.
        let canonical_root = std::fs::canonicalize(&root).unwrap();
        let ctx = ToolContext::new(canonical_root);

        let tool = VilIrDiffTool::new();
        let result = tool
            .execute(
                serde_json::json!({
                    "file": "src.rs",
                    "from": "HEAD~1",
                    "to": "HEAD",
                }),
                &ctx,
            )
            .await
            .expect("tool execute");

        assert_eq!(result["mode"], "rev_range");
        let renames = result["renames"].as_array().expect("renames array");
        assert_eq!(renames.len(), 1, "one rename expected: {result}");
        assert_eq!(renames[0]["old_name"], "foo");
        assert_eq!(renames[0]["new_name"], "bar");

        let summary = result["summary"].as_str().unwrap_or("");
        assert!(
            summary.contains("renamed fn foo → bar"),
            "summary missing rename line: {summary}"
        );

        // No orphan Added/Removed for the renamed pair.
        let changes = result["changes"].as_array().unwrap();
        for c in changes {
            assert_ne!(c["description"], "Added");
            assert_ne!(c["description"], "Removed");
        }
    }

    #[tokio::test]
    async fn worktree_mode_still_works() {
        let (_tmp, root) = init_fixture_repo();
        let src = root.join("src.rs");
        std::fs::write(&src, "pub fn foo(x: i32) -> bool { x > 0 }\n").unwrap();
        git(&root, &["add", "src.rs"]);
        git(&root, &["commit", "-q", "-m", "v1"]);

        // Mutate worktree without committing.
        std::fs::write(&src, "pub fn foo(x: i32, y: i32) -> bool { x > y }\n").unwrap();

        let canonical_root = std::fs::canonicalize(&root).unwrap();
        let ctx = ToolContext::new(canonical_root);

        let tool = VilIrDiffTool::new();
        let result = tool
            .execute(serde_json::json!({ "file": "src.rs" }), &ctx)
            .await
            .expect("tool execute");

        assert_eq!(result["mode"], "rev_vs_worktree");
        assert!(result["total_semantic"].as_u64().unwrap() >= 1);
        let summary = result["summary"].as_str().unwrap_or("");
        assert!(!summary.is_empty(), "expected non-empty summary");
    }
}
