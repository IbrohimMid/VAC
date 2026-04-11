use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct BashInput {
    pub command: String,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct BashOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

pub struct BashTool;

#[allow(clippy::new_without_default)]
impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VilTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the working directory"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory for command"
                },
                "env": {
                    "type": "object",
                    "description": "Environment variables to set"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds"
                }
            },
            "required": ["command"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "Untrusted"
    }

    fn risk_level(&self) -> &str {
        "NeedsApproval"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: BashInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let cwd = if let Some(cwd) = input.cwd {
            if PathBuf::from(&cwd).is_absolute() {
                PathBuf::from(cwd)
            } else {
                context.working_dir.join(cwd)
            }
        } else {
            context.working_dir.clone()
        };

        debug!("Executing bash: {} in {:?}", input.command, cwd);

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&input.command)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(&context.env_vars);

        if let Some(env) = input.env {
            cmd.envs(env);
        }

        if let Some(_timeout) = input.timeout_secs {
            cmd.kill_on_drop(true);
        }

        let output = if let Some(timeout) = input.timeout_secs {
            match tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output()).await
            {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => return Err(ToolError::ExecutionFailed(e.to_string())),
                Err(_) => {
                    return Err(ToolError::ExecutionFailed(format!(
                        "Command timed out after {} seconds",
                        timeout
                    )));
                }
            }
        } else {
            cmd.output()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let success = output.status.success();

        if !success {
            warn!("Bash command failed: exit_code={}", exit_code);
        }

        info!("Bash command completed: exit_code={}", exit_code);

        Ok(serde_json::to_value(BashOutput {
            stdout,
            stderr,
            exit_code,
            success,
        })?)
    }
}
