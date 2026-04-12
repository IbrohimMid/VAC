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
                    "enum": ["status", "diff", "commit", "add", "branch", "log", "pull", "push", "fetch", "checkout"]
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
            _ => {
                return Err(ToolError::InvalidArguments(format!(
                    "Unknown git command: {}",
                    input.command
                )));
            }
        };

        debug!("Running git: {:?}", git_args);

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
