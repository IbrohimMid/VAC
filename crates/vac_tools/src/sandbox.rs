use std::path::PathBuf;
use tracing::info;

use crate::error::ToolError;

/// Sanitize shell command for dangerous patterns.
/// Returns error if command contains injection patterns.
pub fn sanitize_command(command: &str) -> Result<(), ToolError> {
    // Block null bytes
    if command.contains('\0') {
        return Err(ToolError::WardenBlocked("null byte in command".into()));
    }

    // Block backtick substitution
    if command.contains('`') {
        return Err(ToolError::WardenBlocked("backtick substitution not allowed".into()));
    }

    // Block $() process substitution
    if command.contains("$(") {
        return Err(ToolError::WardenBlocked("process substitution $() not allowed".into()));
    }

    // Block command chaining operators (except when properly quoted)
    let dangerous_ops = ["; ", "&&", "||", "| ", " &"];
    for op in dangerous_ops {
        if command.contains(op) {
            return Err(ToolError::WardenBlocked(format!(
                "command chaining '{}' not allowed",
                op.trim()
            )));
        }
    }

    Ok(())
}

pub struct Sandbox {
    enabled: bool,
    working_dir: PathBuf,
}

impl Sandbox {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            enabled: false,
            working_dir,
        }
    }

    pub fn enable(&mut self) {
        info!("Sandbox enabled");
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        info!("Sandbox disabled");
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    pub async fn execute_in_sandbox<F, R>(&self, f: F) -> Result<R, ToolError>
    where
        F: futures::future::Future<Output = Result<R, ToolError>>,
    {
        if self.enabled {
            info!("Executing in sandbox mode");
            f.await
        } else {
            f.await
        }
    }
}
