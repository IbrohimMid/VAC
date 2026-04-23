use async_trait::async_trait;
use glob::glob;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobInput {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobOutput {
    pub pattern: String,
    pub matches: Vec<PathBuf>,
    pub total_matches: usize,
}

pub struct GlobTool;

#[async_trait]
impl crate::registry::VilTool for GlobTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files matching a glob pattern"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"}
            },
            "required": ["pattern"]
        })
    }

    fn trust_requirement(&self) -> &'static str {
        "file_system"
    }

    fn risk_level(&self) -> &'static str {
        "safe"
    }

    /// W2.4 — Same reasoning as grep: defer loading so the initial
    /// manifest stays lean. Pattern-matching over the tree is pulled
    /// on-demand.
    fn should_defer(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &crate::registry::ToolContext,
    ) -> Result<serde_json::Value, crate::error::ToolError> {
        let input: GlobInput = serde_json::from_value(input)?;
        let base_path = input.path.as_deref().unwrap_or(".");
        let root = if PathBuf::from(base_path).is_absolute() {
            PathBuf::from(base_path)
        } else {
            context.working_dir.join(base_path)
        };
        let pattern = root.join(&input.pattern).to_string_lossy().to_string();

        let mut matches = Vec::new();
        for entry in glob(&pattern)? {
            matches.push(entry?);
        }

        let total_matches = matches.len();

        debug!(
            pattern = %input.pattern,
            matches = total_matches,
            "Glob search completed"
        );

        let output = GlobOutput {
            pattern: input.pattern,
            matches,
            total_matches,
        };

        Ok(serde_json::to_value(output)?)
    }
}
