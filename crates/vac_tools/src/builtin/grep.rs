use async_trait::async_trait;
use glob::Pattern;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::SearcherBuilder;
use grep_searcher::sinks::UTF8;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepInput {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub output_mode: Option<String>,
    #[serde(default)]
    pub context_before: Option<usize>,
    #[serde(default)]
    pub context_after: Option<usize>,
    #[serde(default)]
    pub case_insensitive: Option<bool>,
    #[serde(default)]
    pub head_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file: String,
    pub line_number: u64,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepOutput {
    pub pattern: String,
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
}

pub struct GrepTool;

#[async_trait]
impl crate::registry::VilTool for GrepTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search file contents for regex patterns"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"},
                "glob": {"type": "string"},
                "output_mode": {"type": "string"},
                "context_before": {"type": "integer"},
                "context_after": {"type": "integer"},
                "case_insensitive": {"type": "boolean"},
                "head_limit": {"type": "integer"}
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

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &crate::registry::ToolContext,
    ) -> Result<serde_json::Value, crate::error::ToolError> {
        let input: GrepInput = serde_json::from_value(input)?;
        let path = input.path.as_deref().unwrap_or(".");
        let root = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            context.working_dir.join(path)
        };
        let mut matcher_builder = RegexMatcherBuilder::new();
        if let Some(true) = input.case_insensitive {
            matcher_builder.case_insensitive(true);
        }
        let matcher = matcher_builder.build(&input.pattern)?;

        let mut builder = SearcherBuilder::new();
        if let Some(before) = input.context_before {
            builder.before_context(before);
        }
        if let Some(after) = input.context_after {
            builder.after_context(after);
        }

        let mut searcher = builder.build();

        let mut matches = Vec::new();
        let head_limit = input.head_limit.unwrap_or(250);
        let glob_pattern = input.glob.as_deref().map(Pattern::new).transpose()?;

        // Simple recursive search implementation
        let walker = ignore::WalkBuilder::new(&root)
            .standard_filters(true)
            .build();

        for entry in walker {
            let entry = entry?;
            if entry.path().is_file() {
                if let Some(pattern) = &glob_pattern {
                    let relative_path = entry.path().strip_prefix(&root).unwrap_or(entry.path());
                    if !pattern.matches_path(relative_path) {
                        continue;
                    }
                }

                let file = entry.path().display().to_string();
                if let Ok(()) = searcher.search_path(
                    &matcher,
                    entry.path(),
                    UTF8(|line_number, line| {
                        if matches.len() >= head_limit {
                            return Ok(false);
                        }
                        matches.push(GrepMatch {
                            file: file.clone(),
                            line_number,
                            line: line.to_string(),
                        });
                        Ok(true)
                    }),
                ) {
                    // Success
                }
            }
        }

        let total_matches = matches.len();

        debug!(
            pattern = %input.pattern,
            matches = total_matches,
            "Grep search completed"
        );

        let output = GrepOutput {
            pattern: input.pattern,
            matches,
            total_matches,
        };

        Ok(serde_json::to_value(output)?)
    }
}
