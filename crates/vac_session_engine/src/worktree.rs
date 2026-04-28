//! D.3 — Enter/ExitWorktree tool primitives.
//!
//! Wraps `git worktree add` / `git worktree remove` so subagents
//! (Phase B.2) can fork into an isolated checkout, do their
//! mutation work, and merge back without touching the operator's
//! current branch. Worktrees live under
//! `<project_root>/.vac/wt-<id>/`; the session records the path
//! on `AppState.session.worktree` so subsequent tool calls
//! resolve relative to the forked checkout.
//!
//! Safety rails baked in:
//!
//! - `ExitWorktreeTool` refuses to remove a dirty worktree
//!   unless `--force` is set (prevents a subagent from nuking
//!   uncommitted work by accident).
//! - `EnterWorktreeTool` refuses to reuse an existing path;
//!   callers pick a fresh id or call Exit first.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::{EngineError, EngineResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterWorktreeRequest {
    /// Branch to check out in the worktree.
    pub branch: String,
    /// Operator-readable label — surfaces in the Runtime tab
    /// row.
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeHandle {
    pub id: String,
    pub path: PathBuf,
    pub branch: String,
}

/// Enter a new worktree under `project_root/.vac/wt-<uuid>/`.
pub async fn enter_worktree(
    project_root: &Path,
    req: &EnterWorktreeRequest,
) -> EngineResult<WorktreeHandle> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let path = project_root.join(".vac").join(format!("wt-{id}"));
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(EngineError::Other(format!(
            "worktree path {} already exists",
            path.display(),
        )));
    }

    let status = Command::new("git")
        .current_dir(project_root)
        .args([
            "worktree",
            "add",
            path.to_str()
                .ok_or_else(|| EngineError::Other("worktree path not UTF-8".into()))?,
            req.branch.as_str(),
        ])
        .status()
        .await?;
    if !status.success() {
        return Err(EngineError::Other(format!(
            "git worktree add failed: status {:?}",
            status.code(),
        )));
    }

    Ok(WorktreeHandle {
        id,
        path,
        branch: req.branch.clone(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitWorktreeRequest {
    pub path: PathBuf,
    /// If the worktree has uncommitted changes, `force: true` is
    /// required to remove it. Default false matches `git
    /// worktree remove`'s safer semantics.
    #[serde(default)]
    pub force: bool,
}

pub async fn exit_worktree(project_root: &Path, req: &ExitWorktreeRequest) -> EngineResult<()> {
    if !tokio::fs::try_exists(&req.path).await.unwrap_or(false) {
        return Err(EngineError::Other(format!(
            "worktree path {} not found",
            req.path.display(),
        )));
    }
    let mut args = vec!["worktree".to_string(), "remove".into()];
    if req.force {
        args.push("--force".into());
    }
    args.push(
        req.path
            .to_str()
            .ok_or_else(|| EngineError::Other("path not UTF-8".into()))?
            .to_string(),
    );
    let output = Command::new("git")
        .current_dir(project_root)
        .args(&args)
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(EngineError::Other(format!(
            "git worktree remove failed ({}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim(),
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// git repo is not always available in the test env; we
    /// bound the test surface to argument validation + shape
    /// checks.

    #[test]
    fn enter_request_serialises_round_trip() {
        let req = EnterWorktreeRequest {
            branch: "feature/test".into(),
            description: "scratch".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: EnterWorktreeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.branch, back.branch);
    }

    #[tokio::test]
    async fn exit_errors_when_path_missing() {
        let req = ExitWorktreeRequest {
            path: PathBuf::from("/tmp/vac-does-not-exist-deadbeef"),
            force: true,
        };
        let err = exit_worktree(Path::new("/tmp"), &req).await.unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }
}
