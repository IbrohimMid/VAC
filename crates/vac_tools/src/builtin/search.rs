use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchResultKind {
    FileContent,
    Symbol,
    Diagnostic,
    RecentChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MetadataSource {
    Authoritative,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub kind: SearchResultKind,
    pub file: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub score: f64,
    pub snippet: String,
    pub metadata_source: MetadataSource,
}

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResult>,
    pub total: usize,
}

pub struct ContentProvider;
pub struct SymbolProvider;
pub struct DiagnosticProvider;
pub struct RecentChangeProvider;

impl ContentProvider {
    pub async fn search(
        &self,
        root: &Path,
        query: &str,
        file_types: Option<&Vec<String>>,
        max_results: usize,
    ) -> Vec<SearchResult> {
        let query_lower = query.to_ascii_lowercase();
        let mut results = Vec::new();

        for path in walk_files(root, file_types) {
            let Ok(content) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            let display_path = relativize(root, &path);
            for (line_idx, line) in content.lines().enumerate() {
                let line_lower = line.to_ascii_lowercase();
                if let Some(column) = line_lower.find(&query_lower) {
                    results.push(SearchResult {
                        kind: SearchResultKind::FileContent,
                        file: display_path.clone(),
                        line: Some(line_idx + 1),
                        column: Some(column + 1),
                        score: score_line(&query_lower, &line_lower),
                        snippet: line.chars().take(240).collect(),
                        metadata_source: MetadataSource::Authoritative,
                    });
                    if results.len() >= max_results {
                        return results;
                    }
                }
            }
        }

        results
    }
}

impl SymbolProvider {
    pub async fn search(
        &self,
        root: &Path,
        query: &str,
        file_types: Option<&Vec<String>>,
        max_results: usize,
    ) -> Vec<SearchResult> {
        let query_lower = query.to_ascii_lowercase();
        let mut results = Vec::new();

        for path in walk_files(root, file_types) {
            let Ok(content) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            let display_path = relativize(root, &path);
            for (line_idx, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if !looks_like_symbol_definition(trimmed) {
                    continue;
                }
                let lowered = trimmed.to_ascii_lowercase();
                if let Some(column) = lowered.find(&query_lower) {
                    results.push(SearchResult {
                        kind: SearchResultKind::Symbol,
                        file: display_path.clone(),
                        line: Some(line_idx + 1),
                        column: Some(column + 1),
                        score: 0.95,
                        snippet: trimmed.chars().take(240).collect(),
                        metadata_source: MetadataSource::Authoritative,
                    });
                    if results.len() >= max_results {
                        return results;
                    }
                }
            }
        }

        results
    }
}

impl DiagnosticProvider {
    pub async fn search(&self, root: &Path, query: &str, max_results: usize) -> Vec<SearchResult> {
        let Some(cargo_root) = find_cargo_root(root) else {
            return Vec::new();
        };

        let output = Command::new("cargo")
            .current_dir(&cargo_root)
            .args(["check", "--message-format=json", "--quiet"])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };

        let query_lower = query.to_ascii_lowercase();
        let mut results = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if value.get("reason").and_then(|v| v.as_str()) != Some("compiler-message") {
                continue;
            }
            let Some(message) = value.get("message") else {
                continue;
            };
            let rendered = message
                .get("rendered")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let plain_message = message
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !rendered.to_ascii_lowercase().contains(&query_lower)
                && !plain_message.to_ascii_lowercase().contains(&query_lower)
            {
                continue;
            }

            let span = message
                .get("spans")
                .and_then(|v| v.as_array())
                .and_then(|spans| spans.first());
            let file = span
                .and_then(|s| s.get("file_name"))
                .and_then(|v| v.as_str())
                .map(|file| relativize(&cargo_root, Path::new(file)))
                .unwrap_or_else(|| "<unknown>".to_string());
            let line_no = span
                .and_then(|s| s.get("line_start"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let col_no = span
                .and_then(|s| s.get("column_start"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            results.push(SearchResult {
                kind: SearchResultKind::Diagnostic,
                file,
                line: line_no,
                column: col_no,
                score: 0.9,
                snippet: plain_message.to_string(),
                metadata_source: MetadataSource::Authoritative,
            });
            if results.len() >= max_results {
                break;
            }
        }

        results
    }
}

impl RecentChangeProvider {
    pub async fn search(&self, root: &Path, query: &str, max_results: usize) -> Vec<SearchResult> {
        let output = Command::new("git")
            .current_dir(root)
            .args([
                "log",
                "--diff-filter=M",
                "--name-only",
                "-n",
                "50",
                "--pretty=format:",
            ])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        let query_lower = query.to_ascii_lowercase();
        let mut seen = HashSet::new();
        let mut results = Vec::new();
        for file in String::from_utf8_lossy(&output.stdout).lines() {
            let file = file.trim();
            if file.is_empty() || !seen.insert(file.to_string()) {
                continue;
            }
            if !file.to_ascii_lowercase().contains(&query_lower) {
                continue;
            }
            results.push(SearchResult {
                kind: SearchResultKind::RecentChange,
                file: file.to_string(),
                line: None,
                column: None,
                score: 0.8,
                snippet: format!("Recently modified: {file}"),
                metadata_source: MetadataSource::Authoritative,
            });
            if results.len() >= max_results {
                break;
            }
        }
        results
    }
}

pub struct RepoIndexService {
    content_provider: ContentProvider,
    symbol_provider: SymbolProvider,
    diagnostic_provider: DiagnosticProvider,
    recent_change_provider: RecentChangeProvider,
}

impl Default for RepoIndexService {
    fn default() -> Self {
        Self::new()
    }
}

impl RepoIndexService {
    pub fn new() -> Self {
        Self {
            content_provider: ContentProvider,
            symbol_provider: SymbolProvider,
            diagnostic_provider: DiagnosticProvider,
            recent_change_provider: RecentChangeProvider,
        }
    }

    pub async fn search(
        &self,
        root: &Path,
        query: &str,
        file_types: Option<&Vec<String>>,
        max_results: usize,
    ) -> Vec<SearchResult> {
        let budget = max_results.max(1);
        let mut results = Vec::new();

        results.extend(
            self.content_provider
                .search(root, query, file_types, budget)
                .await,
        );
        results.extend(
            self.symbol_provider
                .search(root, query, file_types, budget)
                .await,
        );
        results.extend(self.diagnostic_provider.search(root, query, budget).await);
        results.extend(
            self.recent_change_provider
                .search(root, query, budget)
                .await,
        );

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
        results.truncate(max_results);
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
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn search_read_classification(
        &self,
        _args: &serde_json::Value,
    ) -> crate::registry::SearchReadKind {
        crate::registry::SearchReadKind::Search
    }

    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search code, symbols, diagnostics, and recent changes"
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

        let max_results = input.max_results.unwrap_or(50).max(1);
        debug!("Searching for '{}' in {:?}", input.query, search_path);

        let results = self
            .index_service
            .search(
                &search_path,
                &input.query,
                input.file_types.as_ref(),
                max_results,
            )
            .await;

        let total = results.len();
        info!("Search completed: {} results found", total);

        Ok(serde_json::to_value(SearchOutput { results, total })?)
    }
}

fn walk_files(root: &Path, file_types: Option<&Vec<String>>) -> Vec<PathBuf> {
    let normalized_types = file_types.map(|types| {
        types
            .iter()
            .map(|ty| ty.trim_start_matches('.').to_ascii_lowercase())
            .collect::<HashSet<_>>()
    });

    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | "target" | "node_modules")
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| match &normalized_types {
            Some(types) => entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| types.contains(&ext.to_ascii_lowercase()))
                .unwrap_or(false),
            None => true,
        })
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

fn relativize(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn score_line(query_lower: &str, line_lower: &str) -> f64 {
    if line_lower == query_lower {
        1.0
    } else if line_lower.starts_with(query_lower) {
        0.98
    } else {
        0.88
    }
}

fn looks_like_symbol_definition(trimmed: &str) -> bool {
    [
        "pub fn ",
        "fn ",
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "impl ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn find_cargo_root(root: &Path) -> Option<PathBuf> {
    for candidate in root.ancestors() {
        if candidate.join("Cargo.toml").exists() {
            return Some(candidate.to_path_buf());
        }
    }
    if root.join("Cargo.toml").exists() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(root)
            .env("GIT_AUTHOR_NAME", "VAC Test")
            .env("GIT_AUTHOR_EMAIL", "vac-test@example.com")
            .env("GIT_COMMITTER_NAME", "VAC Test")
            .env("GIT_COMMITTER_EMAIL", "vac-test@example.com")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn search_result_schema_roundtrip_json() {
        let result = SearchResult {
            kind: SearchResultKind::Symbol,
            file: "src/lib.rs".to_string(),
            line: Some(12),
            column: Some(5),
            score: 0.95,
            snippet: "pub fn hello() {}".to_string(),
            metadata_source: MetadataSource::Authoritative,
        };

        let json = serde_json::to_string(&result).unwrap();
        let decoded: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, result);
    }

    #[tokio::test]
    async fn symbol_provider_finds_pub_fn() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn hello_world() {}\nfn hidden() {}\n",
        )
        .unwrap();

        let results = SymbolProvider
            .search(dir.path(), "hello_world", Some(&vec!["rs".to_string()]), 10)
            .await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, SearchResultKind::Symbol);
        assert_eq!(results[0].file, "src/lib.rs");
        assert_eq!(results[0].line, Some(1));
        assert!(results[0].snippet.contains("pub fn hello_world"));
    }

    #[tokio::test]
    async fn recent_change_provider_returns_modified_files() {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init"]);
        std::fs::write(dir.path().join("sample.rs"), "fn a() {}\n").unwrap();
        git(dir.path(), &["add", "sample.rs"]);
        git(dir.path(), &["commit", "-m", "initial"]);

        std::fs::write(dir.path().join("sample.rs"), "fn b() {}\n").unwrap();
        git(dir.path(), &["commit", "-am", "modify sample"]);

        let results = RecentChangeProvider.search(dir.path(), "sample", 10).await;
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, SearchResultKind::RecentChange);
        assert_eq!(results[0].file, "sample.rs");
    }
}
