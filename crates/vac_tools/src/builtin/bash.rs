use async_trait::async_trait;
use chrono;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

use crate::approvals::{ShellApprovalPolicy, default_policy};
use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct BashInput {
    pub command: String,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub run_in_background: bool,
    #[serde(default)]
    pub session_id: Option<String>,
    pub description: Option<String>,
}

fn default_timeout_secs() -> u64 {
    600
}

#[derive(Debug)]
#[allow(dead_code)]
struct ShellSession {
    child: Child,
    id: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Default)]
pub struct BashTool {
    #[allow(dead_code)]
    sessions: Arc<DashMap<String, ShellSession>>,
}

#[derive(Debug, Serialize)]
pub struct BashOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

#[allow(clippy::new_without_default)]
impl BashTool {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl VilTool for BashTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn inputs_equivalent(&self, a: &serde_json::Value, b: &serde_json::Value) -> bool {
        // Dedups identical command strings
        let a_cmd = a.get("command").and_then(|v| v.as_str());
        let b_cmd = b.get("command").and_then(|v| v.as_str());
        match (a_cmd, b_cmd) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

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
        "dangerous"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: BashInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Warden guardrails: sanitize command for injection patterns
        crate::sandbox::sanitize_command(&input.command)?;

        // Hierarchical approval check
        let policy = load_shell_policy(&context.working_dir);
        if let Some(reason) = policy.is_denied(&input.command) {
            warn!(command = %input.command, reason = %reason, "Bash command denied by scope policy");
            return Err(ToolError::PermissionDenied(format!(
                "Command `{}` {}",
                input.command, reason
            )));
        }

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

        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                // Apply 2GB memory limit
                let _ = crate::resource_limits::apply_rlimit_as(2 * 1024 * 1024 * 1024);
                // Apply 1GB file size limit (disk quota)
                let _ = crate::resource_limits::apply_rlimit_fsize(1024 * 1024 * 1024);
                Ok(())
            });
        }

        cmd.kill_on_drop(true);

        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(input.timeout_secs),
            cmd.output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(ToolError::ExecutionFailed(e.to_string())),
            Err(_) => {
                return Err(ToolError::ExecutionFailed(format!(
                    "Command timed out after {} seconds",
                    input.timeout_secs
                )));
            }
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

/// Load shell approval policy from .vac/config.toml [approvals], falling back to default.
fn load_shell_policy(working_dir: &std::path::Path) -> ShellApprovalPolicy {
    let config_path = working_dir.join(".vac/config.toml");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(table) = content.parse::<toml::Table>() {
            return ShellApprovalPolicy::from_toml(&table);
        }
    }
    default_policy()
}
