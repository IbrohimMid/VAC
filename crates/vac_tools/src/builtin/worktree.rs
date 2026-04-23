//! F1.3 — `enter_worktree` + `exit_worktree` tools.
//!
//! Claude Code lesson: git worktree is a powerful isolation primitive,
//! but TUIs don't expose it. As a pair of tools the agent can spin up
//! a side branch for experiments, run VIL tools safely, and either
//! merge or discard without touching the operator's main worktree.
//!
//! Implementation wraps `git worktree add|remove` and records the
//! active worktree to `.vac/worktree.lock` so autopilot/bridge can
//! observe state.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::process::Command;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

const LOCK_FILE: &str = ".vac/worktree.lock";
/// Git worktree operations shouldn't take more than ~30s in practice.
/// Cap protects against hangs (auth prompts, network FS, stuck git hook).
const GIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Reject absolute paths or any component containing `..`. Result
/// must remain inside the project `working_dir`.
fn validate_relative_path(path: &str) -> Result<(), ToolError> {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return Err(ToolError::ExecutionFailed(format!(
            "worktree path must be relative to working_dir, got absolute: {path}"
        )));
    }
    for component in p.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(ToolError::ExecutionFailed(format!(
                "worktree path must not contain '..': {path}"
            )));
        }
    }
    Ok(())
}

/// Branch names that would make git angry or let the caller escape.
fn validate_branch_name(branch: &str) -> Result<(), ToolError> {
    if branch.is_empty() {
        return Err(ToolError::ExecutionFailed("branch name empty".into()));
    }
    if branch.starts_with('-') {
        return Err(ToolError::ExecutionFailed(format!(
            "branch must not start with '-' (would be interpreted as git flag): {branch}"
        )));
    }
    // git refuses these anyway but reject early with a clearer message.
    for bad in ["..", "//", "@{", "\\"] {
        if branch.contains(bad) {
            return Err(ToolError::ExecutionFailed(format!(
                "branch contains forbidden sequence '{bad}': {branch}"
            )));
        }
    }
    if branch.len() > 200 {
        return Err(ToolError::ExecutionFailed(format!(
            "branch name too long ({} > 200)",
            branch.len()
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct EnterInput {
    /// Branch name to create + check out in the new worktree.
    branch: String,
    /// Base revision (default: `HEAD`).
    #[serde(default)]
    base: Option<String>,
    /// Directory under project root (default: `.vac/worktrees/<branch>`).
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct EnterOutput {
    worktree_path: String,
    branch: String,
    base: String,
}

pub struct EnterWorktreeTool;

impl EnterWorktreeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnterWorktreeTool {
    fn default() -> Self {
        Self::new()
    }
}

fn sanitize_branch_dir(branch: &str) -> String {
    branch.replace('/', "-").replace(['\\', ':', '*', '?', '"', '<', '>', '|'], "-")
}

#[async_trait]
impl VilTool for EnterWorktreeTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "enter_worktree"
    }

    fn description(&self) -> &str {
        "Create a git worktree for experimental/isolated work. Useful for agent-driven refactors that shouldn't touch the operator's main working copy until merged."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "branch": { "type": "string", "description": "Branch name (will be created)." },
                "base": { "type": "string", "description": "Base revision (default: HEAD)." },
                "path": { "type": "string", "description": "Path under project root (default: .vac/worktrees/<branch>)." }
            },
            "required": ["branch"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "ask_once"
    }

    fn risk_level(&self) -> &str {
        "mutating"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: EnterInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        validate_branch_name(&input.branch)?;

        let base = input.base.unwrap_or_else(|| "HEAD".to_string());
        validate_branch_name(&base)?;

        let path = match input.path {
            Some(p) => {
                validate_relative_path(&p)?;
                p
            }
            None => format!(".vac/worktrees/{}", sanitize_branch_dir(&input.branch)),
        };

        let abs_path = context.working_dir.join(&path);
        if let Some(parent) = abs_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("create parent dir: {e}")))?;
        }

        let cmd_fut = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &input.branch,
                abs_path.to_str().unwrap_or(&path),
                &base,
            ])
            .current_dir(&context.working_dir)
            .output();
        let output = tokio::time::timeout(GIT_TIMEOUT, cmd_fut)
            .await
            .map_err(|_| {
                ToolError::ExecutionFailed(format!(
                    "git worktree add timed out after {}s",
                    GIT_TIMEOUT.as_secs()
                ))
            })?
            .map_err(|e| ToolError::ExecutionFailed(format!("git worktree add: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::ExecutionFailed(format!(
                "git worktree add failed: {stderr}"
            )));
        }

        let lock_path = context.working_dir.join(LOCK_FILE);
        if let Some(parent) = lock_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("create lock parent: {e}")))?;
        }
        let lock_body = serde_json::json!({
            "worktree_path": abs_path.to_string_lossy(),
            "branch": input.branch,
            "base": base,
            "entered_at_epoch_s": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
        let bytes = serde_json::to_vec_pretty(&lock_body)
            .map_err(|e| ToolError::ExecutionFailed(format!("serialize lock: {e}")))?;
        atomic_write(&lock_path, &bytes).await?;

        let out = EnterOutput {
            worktree_path: abs_path.to_string_lossy().to_string(),
            branch: input.branch,
            base,
        };
        serde_json::to_value(out).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

/// Atomic file write: write to `<path>.tmp` then rename. Prevents
/// torn/corrupt files if the process dies mid-write.
pub(crate) async fn atomic_write(
    path: &std::path::Path,
    bytes: &[u8],
) -> Result<(), ToolError> {
    let tmp = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    tokio::fs::write(&tmp, bytes)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("write tmp: {e}")))?;
    tokio::fs::rename(&tmp, path)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("rename: {e}")))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ExitInput {
    /// Path or branch name of worktree to remove.
    #[serde(default)]
    worktree: Option<String>,
    /// Force removal even if uncommitted changes (default: false).
    #[serde(default)]
    force: bool,
}

pub struct ExitWorktreeTool;

impl ExitWorktreeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExitWorktreeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for ExitWorktreeTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "exit_worktree"
    }

    fn description(&self) -> &str {
        "Remove a git worktree previously created via enter_worktree. Defaults to the active worktree tracked in .vac/worktree.lock."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "worktree": { "type": "string", "description": "Path or branch to remove; defaults to active worktree." },
                "force": { "type": "boolean", "description": "Force removal even with uncommitted changes." }
            },
            "required": []
        })
    }

    fn trust_requirement(&self) -> &str {
        "ask_once"
    }

    fn risk_level(&self) -> &str {
        "destructive"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: ExitInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        let lock_path = context.working_dir.join(LOCK_FILE);
        let target = match input.worktree {
            Some(p) => p,
            None => {
                let content = tokio::fs::read_to_string(&lock_path).await.map_err(|_| {
                    ToolError::ExecutionFailed(
                        "no active worktree (missing worktree arg and no lock file)".into(),
                    )
                })?;
                let doc: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
                    ToolError::ExecutionFailed(format!("malformed lock: {e}"))
                })?;
                doc["worktree_path"]
                    .as_str()
                    .ok_or_else(|| {
                        ToolError::ExecutionFailed(
                            "lock file missing worktree_path".into(),
                        )
                    })?
                    .to_string()
            }
        };

        // Defense-in-depth: reject target that starts with `-`; blocks
        // someone tricking exit into running `git worktree remove --exec…`.
        if target.starts_with('-') {
            return Err(ToolError::ExecutionFailed(format!(
                "worktree target must not start with '-': {target}"
            )));
        }

        let mut cmd = Command::new("git");
        cmd.args(["worktree", "remove"]);
        if input.force {
            cmd.arg("--force");
        }
        cmd.arg("--").arg(&target);
        let cmd_fut = cmd.current_dir(&context.working_dir).output();
        let output = tokio::time::timeout(GIT_TIMEOUT, cmd_fut)
            .await
            .map_err(|_| {
                ToolError::ExecutionFailed(format!(
                    "git worktree remove timed out after {}s",
                    GIT_TIMEOUT.as_secs()
                ))
            })?
            .map_err(|e| ToolError::ExecutionFailed(format!("git worktree remove: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::ExecutionFailed(format!(
                "git worktree remove failed: {stderr}"
            )));
        }

        let lock_removed = tokio::fs::remove_file(&lock_path).await.is_ok();
        let out = serde_json::json!({
            "removed_path": target,
            "forced": input.force,
            "lock_removed": lock_removed,
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_branch_dir_strips_path_separators() {
        assert_eq!(sanitize_branch_dir("feat/foo"), "feat-foo");
        assert_eq!(sanitize_branch_dir("a:b"), "a-b");
        assert_eq!(sanitize_branch_dir("clean"), "clean");
    }

    #[test]
    fn validate_relative_path_rejects_absolute() {
        assert!(validate_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn validate_relative_path_rejects_parent_component() {
        assert!(validate_relative_path("../../escape").is_err());
        assert!(validate_relative_path(".vac/../../escape").is_err());
    }

    #[test]
    fn validate_relative_path_accepts_subdir() {
        assert!(validate_relative_path(".vac/worktrees/ok").is_ok());
        assert!(validate_relative_path("subdir/nested").is_ok());
    }

    #[test]
    fn validate_branch_name_rejects_flag_prefix() {
        assert!(validate_branch_name("-exec=rm").is_err());
    }

    #[test]
    fn validate_branch_name_rejects_dot_dot_and_forbidden_sequences() {
        assert!(validate_branch_name("feat/..hack").is_err());
        assert!(validate_branch_name("a//b").is_err());
        assert!(validate_branch_name("a@{x").is_err());
        assert!(validate_branch_name("path\\with\\back").is_err());
    }

    #[test]
    fn validate_branch_name_rejects_empty_and_oversize() {
        assert!(validate_branch_name("").is_err());
        let long: String = "x".repeat(201);
        assert!(validate_branch_name(&long).is_err());
    }

    #[test]
    fn validate_branch_name_accepts_normal_branches() {
        assert!(validate_branch_name("feat/new").is_ok());
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("release-1.2").is_ok());
    }

    #[test]
    fn enter_input_validates_required_branch() {
        let r: Result<EnterInput, _> = serde_json::from_value(serde_json::json!({}));
        assert!(r.is_err(), "missing branch field must fail");
    }

    #[test]
    fn enter_input_parses_optional_fields() {
        let i: EnterInput = serde_json::from_value(serde_json::json!({
            "branch": "feat/new",
            "base": "main",
        }))
        .unwrap();
        assert_eq!(i.branch, "feat/new");
        assert_eq!(i.base.as_deref(), Some("main"));
        assert!(i.path.is_none());
    }
}
