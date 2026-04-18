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
#[serde(tag = "kind")]
pub enum SearchResult {
    PathMatch { path: String },
    ContentMatch { file: String, line: usize, content: String },
    SymbolMatch { symbol: String, file: String, line: usize },
    DiagnosticMatch { message: String, file: String, line: usize, severity: String },
    RecentChange { file: String, change_type: String },
}

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResult>,
    pub total: usize,
}

pub struct RepoIndexService {
    // We can store indexes here, but for now we implement on-the-fly
    // to match the previous tool's behavior, while providing the new rich structure.
}

impl RepoIndexService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn search_path(&self, root: &PathBuf, query: &str, max_results: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let walker = walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
            let p = e.path().to_string_lossy();
            !p.contains(".git") && !p.contains("target") && !p.contains("node_modules")
        });

        for entry in walker.filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path_str = entry.path().to_string_lossy().to_string();
                if path_str.to_lowercase().contains(&query.to_lowercase()) {
                    results.push(SearchResult::PathMatch { path: path_str });
                    if results.len() >= max_results { break; }
                }
            }
        }
        results
    }

    pub async fn search_content(&self, root: &PathBuf, query: &str, file_types: Option<&Vec<String>>, max_results: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let walker = walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
            let p = e.path().to_string_lossy();
            !p.contains(".git") && !p.contains("target") && !p.contains("node_modules")
        });

        for entry in walker.filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(types) = file_types {
                    if let Some(ext) = path.extension() {
                        if !types.contains(&ext.to_string_lossy().to_string()) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    for (line_num, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&query.to_lowercase()) {
                            results.push(SearchResult::ContentMatch {
                                file: path.to_string_lossy().to_string(),
                                line: line_num + 1,
                                content: line.chars().take(200).collect(),
                            });
                            if results.len() >= max_results { return results; }
                        }
                    }
                }
            }
        }
        results
    }
}

pub struct SearchTool {
    index_service: std::sync::Arc<RepoIndexService>,
}

#[allow(clippy::new_without_default)]
impl SearchTool {
    pub fn new() -> Self {
        Self {
            index_service: std::sync::Arc::new(RepoIndexService::new()),
        }
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

        // Combine path search and content search for unified search
        let mut results = self.index_service.search_path(&search_path, &input.query, max_results / 2).await;
        
        let content_results = self.index_service.search_content(&search_path, &input.query, input.file_types.as_ref(), max_results - results.len()).await;
        results.extend(content_results);

        let total = results.len();
        info!("Search completed: {} results found", total);

        Ok(serde_json::to_value(SearchOutput { results, total })?)
    }
}
