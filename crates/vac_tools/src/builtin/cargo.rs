use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct CargoInput {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub manifest_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CargoOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

pub struct CargoTool;

#[allow(clippy::new_without_default)]
impl CargoTool {
    pub fn new() -> Self {
        Self
    }

    async fn run_cargo_command(
        &self,
        cwd: &PathBuf,
        manifest_path: Option<&str>,
        args: &[String],
    ) -> Result<CargoOutput, ToolError> {
        let mut cmd = Command::new("cargo");
        cmd.args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(manifest) = manifest_path {
            cmd.arg("--manifest-path").arg(manifest);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let success = output.status.success();

        Ok(CargoOutput {
            stdout,
            stderr,
            exit_code,
            success,
        })
    }
}

#[async_trait]
impl VilTool for CargoTool {
    fn name(&self) -> &str {
        "cargo"
    }

    fn description(&self) -> &str {
        "Execute cargo commands (build, test, clippy, check, etc.)"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Cargo subcommand to run",
                    "enum": ["build", "check", "test", "clippy", "fmt", "run", "bench", "doc"]
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments for cargo"
                },
                "manifest_path": {
                    "type": "string",
                    "description": "Path to Cargo.toml"
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
        let input: CargoInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let mut cargo_args = vec![input.command.clone()];

        if let Some(additional) = &input.args {
            cargo_args.extend(additional.clone());
        }

        let manifest_path = input.manifest_path.as_deref();

        debug!(
            "Running cargo: {:?} in {:?}",
            cargo_args, context.working_dir
        );

        let output = self
            .run_cargo_command(&context.working_dir, manifest_path, &cargo_args)
            .await?;

        info!(
            "Cargo command '{}' completed: exit_code={}",
            input.command, output.exit_code
        );

        serde_json::to_value(output).map_err(ToolError::SerializationError)
    }
}
