//! canonical_lint — agent-callable tool that lints text or a file for
//! legacy VIL terminology using `vil_knowledge::lint_text`.
//!
//! Exactly one of `file_path` or `text` must be provided. `mode` defaults to
//! `"strict"` (legacy terms reported as errors); pass `"compat"` to downgrade
//! them to warnings.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use crate::security::validate_path_within_root;
use vil_knowledge::{CanonicalMode, CanonicalViolation, lint_text};

#[derive(Debug, Deserialize)]
struct LintInput {
    /// Path to a file to lint (relative to project root).
    #[serde(default)]
    file_path: Option<String>,
    /// Inline text to lint instead of reading a file.
    #[serde(default)]
    text: Option<String>,
    /// "strict" (default) or "compat". Strict emits errors, compat emits warnings.
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct LintOutput {
    violations: Vec<ViolationEntry>,
    violation_count: usize,
    mode: String,
    source: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct ViolationEntry {
    span_start: usize,
    span_end: usize,
    legacy_term: String,
    suggested_replacement: String,
    severity: String,
}

impl From<&CanonicalViolation> for ViolationEntry {
    fn from(v: &CanonicalViolation) -> Self {
        Self {
            span_start: v.span.0,
            span_end: v.span.1,
            legacy_term: v.legacy_term.clone(),
            suggested_replacement: v.suggested_replacement.clone(),
            severity: match v.severity {
                vil_knowledge::CanonicalSeverity::Error => "error".to_string(),
                vil_knowledge::CanonicalSeverity::Warning => "warning".to_string(),
            },
        }
    }
}

pub struct CanonicalLintTool;

impl CanonicalLintTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CanonicalLintTool {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_mode(raw: Option<&str>) -> Result<CanonicalMode, ToolError> {
    match raw.unwrap_or("strict").to_ascii_lowercase().as_str() {
        "strict" => Ok(CanonicalMode::Strict),
        "compat" => Ok(CanonicalMode::Compat),
        other => Err(ToolError::InvalidArguments(format!(
            "mode must be 'strict' or 'compat', got '{other}'"
        ))),
    }
}

fn mode_label(mode: CanonicalMode) -> &'static str {
    match mode {
        CanonicalMode::Strict => "strict",
        CanonicalMode::Compat => "compat",
    }
}

fn build_summary(violations: &[CanonicalViolation], source_label: &str) -> String {
    if violations.is_empty() {
        return format!("canonical_lint: {source_label} clean — no legacy VIL terms detected");
    }
    let mut lines = Vec::with_capacity(violations.len() + 1);
    lines.push(format!(
        "canonical_lint: {source_label} — {} violation(s)",
        violations.len()
    ));
    for v in violations {
        lines.push(format!(
            "  [{sev:?}] bytes {start}..{end}: '{legacy}' → '{canonical}'",
            sev = v.severity,
            start = v.span.0,
            end = v.span.1,
            legacy = v.legacy_term,
            canonical = v.suggested_replacement,
        ));
    }
    lines.join("\n")
}

#[async_trait]
impl VilTool for CanonicalLintTool {
    fn name(&self) -> &str {
        "canonical_lint"
    }

    fn description(&self) -> &str {
        "Lint text or a file for legacy VIL terminology (v-cel, VRule, VxApp) \
        and suggest canonical replacements (vil-expr, Rule, VilServer). Provide \
        exactly one of 'file_path' or 'text'. Mode is 'strict' (errors) or \
        'compat' (warnings)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File path relative to project root. Mutually exclusive with 'text'."
                },
                "text": {
                    "type": "string",
                    "description": "Inline text to lint. Mutually exclusive with 'file_path'."
                },
                "mode": {
                    "type": "string",
                    "enum": ["strict", "compat"],
                    "description": "Severity mode (default: 'strict'). Strict reports errors; compat reports warnings."
                }
            }
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: LintInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let mode = parse_mode(input.mode.as_deref())?;

        let (content, source_label) = match (input.file_path.as_ref(), input.text.as_ref()) {
            (Some(_), Some(_)) => {
                return Err(ToolError::InvalidArguments(
                    "Provide exactly one of 'file_path' or 'text', not both".into(),
                ));
            }
            (None, None) => {
                return Err(ToolError::InvalidArguments(
                    "Provide exactly one of 'file_path' or 'text'".into(),
                ));
            }
            (Some(path), None) => {
                let abs = validate_path_within_root(&context.working_dir, path)?;
                let content = std::fs::read_to_string(&abs).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to read {path}: {e}"))
                })?;
                (content, format!("file:{path}"))
            }
            (None, Some(text)) => (text.clone(), "text".to_string()),
        };

        let violations = lint_text(&content, mode);
        let entries: Vec<ViolationEntry> = violations.iter().map(ViolationEntry::from).collect();
        let summary = build_summary(&violations, &source_label);

        let output = LintOutput {
            violation_count: entries.len(),
            violations: entries,
            mode: mode_label(mode).to_string(),
            source: source_label,
            summary,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ToolContext;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext::new(PathBuf::from("."))
    }

    #[tokio::test]
    async fn tool_lints_inline_text_strict() {
        let tool = CanonicalLintTool::new();
        let args = serde_json::json!({
            "text": "language: v-cel\nkind: VxApp",
            "mode": "strict"
        });
        let result = tool.execute(args, &ctx()).await.expect("execute");
        assert_eq!(result["violation_count"], 2);
        assert_eq!(result["mode"], "strict");
        assert_eq!(result["violations"][0]["severity"], "error");
        // Both violations present
        let terms: Vec<String> = result["violations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["legacy_term"].as_str().unwrap().to_string())
            .collect();
        assert!(terms.iter().any(|t| t == "v-cel"));
        assert!(terms.iter().any(|t| t == "VxApp"));
    }

    #[tokio::test]
    async fn tool_clean_text_reports_zero() {
        let tool = CanonicalLintTool::new();
        let args = serde_json::json!({
            "text": "language: vil-expr\nkind: VilServer",
        });
        let result = tool.execute(args, &ctx()).await.expect("execute");
        assert_eq!(result["violation_count"], 0);
        assert_eq!(result["mode"], "strict");
        assert_eq!(result["violations"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn tool_compat_mode_emits_warnings() {
        let tool = CanonicalLintTool::new();
        let args = serde_json::json!({
            "text": "VRule alias",
            "mode": "compat",
        });
        let result = tool.execute(args, &ctx()).await.expect("execute");
        assert_eq!(result["violation_count"], 1);
        assert_eq!(result["mode"], "compat");
        assert_eq!(result["violations"][0]["severity"], "warning");
    }

    #[tokio::test]
    async fn tool_rejects_both_inputs() {
        let tool = CanonicalLintTool::new();
        let args = serde_json::json!({
            "text": "x",
            "file_path": "y",
        });
        let err = tool.execute(args, &ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn tool_rejects_missing_inputs() {
        let tool = CanonicalLintTool::new();
        let err = tool
            .execute(serde_json::json!({}), &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn tool_rejects_bad_mode() {
        let tool = CanonicalLintTool::new();
        let args = serde_json::json!({
            "text": "x",
            "mode": "banana",
        });
        let err = tool.execute(args, &ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn tool_lints_file_path() {
        let tool = CanonicalLintTool::new();
        let tmp = tempfile::tempdir().expect("tempdir");
        let file_rel = "dirty.txt";
        let abs = tmp.path().join(file_rel);
        std::fs::write(&abs, "language: v-cel and kind: VxApp").unwrap();

        let mut context = ctx();
        context.working_dir = tmp.path().to_path_buf();

        let args = serde_json::json!({ "file_path": file_rel });
        let result = tool.execute(args, &context).await.expect("execute");
        assert_eq!(result["violation_count"], 2);
        assert!(result["source"].as_str().unwrap().starts_with("file:"));
    }

    #[test]
    fn summary_is_human_readable() {
        let violations = vec![CanonicalViolation {
            span: (0, 5),
            legacy_term: "VRule".into(),
            suggested_replacement: "Rule".into(),
            severity: vil_knowledge::CanonicalSeverity::Error,
        }];
        let s = build_summary(&violations, "text");
        assert!(s.contains("1 violation"));
        assert!(s.contains("VRule"));
        assert!(s.contains("Rule"));
    }
}
