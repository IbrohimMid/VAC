use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub path: Option<String>,
    pub file_types: Option<Vec<String>>,
    pub max_results: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResult>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub file: String,
    pub line: usize,
    pub content: String,
}

pub struct SearchTool;

#[allow(clippy::new_without_default)]
impl SearchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VilTool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search for text patterns in files"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query string"
                },
                "path": {
                    "type": "string",
                    "description": "Directory path to search in"
                },
                "file_types": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "File extensions to search"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results",
                    "default": 50
                }
            },
            "required": ["query"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "Trusted"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: SearchInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let search_path = if let Some(path) = input.path {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                context.working_dir.join(path)
            }
        } else {
            context.working_dir.clone()
        };

        let max_results = input.max_results.unwrap_or(50);

        debug!("Searching for '{}' in {:?}", input.query, search_path);

        let mut results = Vec::new();

        let walker = walkdir::WalkDir::new(&search_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let path = e.path();
                !path.to_string_lossy().contains(".git")
                    && !path.to_string_lossy().contains("target")
                    && !path.to_string_lossy().contains("node_modules")
            });

        for entry in walker.filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path();

                if let Some(ref types) = input.file_types {
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_string();
                        if !types.contains(&ext_str) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    for (line_num, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&input.query.to_lowercase()) {
                            results.push(SearchResult {
                                file: path.to_string_lossy().to_string(),
                                line: line_num + 1,
                                content: line.chars().take(200).collect(),
                            });

                            if results.len() >= max_results {
                                break;
                            }
                        }
                    }
                }
            }

            if results.len() >= max_results {
                break;
            }
        }

        let total = results.len();
        info!("Search completed: {} results found", total);

        Ok(serde_json::to_value(SearchOutput { results, total })?)
    }
}
