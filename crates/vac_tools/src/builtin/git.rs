use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct GitInput {
    pub command: String,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct GitStatusEntry {
    pub path: String,
    pub status: String,
    pub staged: bool,
}

#[derive(Debug, Serialize)]
pub struct GitStatusOutput {
    pub branch: String,
    pub entries: Vec<GitStatusEntry>,
    pub raw_output: GitOutput,
}

#[derive(Debug, Serialize)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

pub struct GitTool;

#[allow(clippy::new_without_default)]
impl GitTool {
    pub fn new() -> Self {
        Self
    }

    async fn run_git_command(
        &self,
        cwd: &PathBuf,
        args: &[String],
    ) -> Result<GitOutput, ToolError> {
        let mut cmd = Command::new("git");
        cmd.args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let success = output.status.success();

        Ok(GitOutput {
            stdout,
            stderr,
            exit_code,
            success,
        })
    }

    pub async fn get_typed_status(&self, cwd: &PathBuf) -> Result<GitStatusOutput, ToolError> {
        let raw = self
            .run_git_command(
                cwd,
                &[
                    "status".to_string(),
                    "--porcelain=v1".to_string(),
                    "-b".to_string(),
                ],
            )
            .await?;

        let mut entries = Vec::new();
        let mut branch = String::new();

        for line in raw.stdout.lines() {
            if line.starts_with("##") {
                branch = line[3..].split("...").next().unwrap_or("").to_string();
                continue;
            }
            if line.len() > 3 {
                let status = &line[0..2];
                let path = line[3..].trim().to_string();
                let staged = status.starts_with('A')
                    || status.starts_with('M')
                    || status.starts_with('D')
                    || status.starts_with('R')
                    || status.starts_with('C');
                entries.push(GitStatusEntry {
                    path,
                    status: status.to_string(),
                    staged,
                });
            }
        }

        Ok(GitStatusOutput {
            branch,
            entries,
            raw_output: raw,
        })
    }
}

#[async_trait]
impl VilTool for GitTool {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Execute git commands in the repository"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Git subcommand (status, diff, commit, etc.)",
                    "enum": ["status", "diff", "commit", "add", "branch", "log", "pull", "push", "fetch", "checkout", "stash", "worktree"]
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments for the git command"
                }
            },
            "required": ["command"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "Trusted"
    }

    fn risk_level(&self) -> &str {
        "needs_approval"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: GitInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let git_args = match input.command.as_str() {
            "status" => vec!["status".to_string()],
            "diff" => vec!["diff".to_string()],
            "log" => vec![
                "log".to_string(),
                "--oneline".to_string(),
                "-10".to_string(),
            ],
            "branch" => vec!["branch".to_string(), "-a".to_string()],
            "fetch" => vec!["fetch".to_string(), "--all".to_string()],
            "pull" => vec!["pull".to_string()],
            "add" => {
                let mut base = vec!["add".to_string()];
                if let Some(additional) = input.args {
                    base.extend(additional);
                }
                base
            }
            "commit" => {
                let mut base = vec!["commit".to_string(), "-m".to_string()];
                if let Some(additional) = input.args {
                    base.extend(additional);
                }
                base
            }
            "push" => vec!["push".to_string()],
            "checkout" => {
                let mut base = vec!["checkout".to_string()];
                if let Some(additional) = input.args {
                    base.extend(additional);
                }
                base
            }
            "stash" => {
                let mut base = vec!["stash".to_string()];
                if let Some(additional) = input.args {
                    base.extend(additional);
                }
                base
            }
            "worktree" => {
                let mut base = vec!["worktree".to_string()];
                if let Some(additional) = input.args {
                    base.extend(additional);
                }
                base
            }
            _ => {
                return Err(ToolError::InvalidArguments(format!(
                    "Unknown git command: {}",
                    input.command
                )));
            }
        };

        debug!("Running git: {:?}", git_args);

        if input.command == "status" {
            let status_output = self.get_typed_status(&context.working_dir).await?;
            info!(
                "Git command '{}' completed: exit_code={}",
                input.command, status_output.raw_output.exit_code
            );
            return Ok(serde_json::json!({
                "success": status_output.raw_output.exit_code == 0,
                "branch": status_output.branch,
                "entries": status_output.entries,
                "raw_output": status_output.raw_output,
            }));
        }

        let output = self
            .run_git_command(&context.working_dir, &git_args)
            .await?;
        info!(
            "Git command '{}' completed: exit_code={}",
            input.command, output.exit_code
        );

        serde_json::to_value(output).map_err(ToolError::SerializationError)
    }
}
