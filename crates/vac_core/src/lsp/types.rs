use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LspSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl LspSeverity {
    pub fn from_lsp_code(code: u32) -> Self {
        match code {
            1 => Self::Error,
            2 => Self::Warning,
            3 => Self::Information,
            _ => Self::Hint,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
    pub file_path: PathBuf,
    pub severity: LspSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
    pub range: LspRange,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspWorkspaceSnapshot {
    pub diagnostics: Vec<LspDiagnostic>,
    pub total_errors: usize,
    pub total_warnings: usize,
}

impl LspWorkspaceSnapshot {
    pub fn rebuild_counts(&mut self) {
        self.total_errors = self
            .diagnostics
            .iter()
            .filter(|d| d.severity == LspSeverity::Error)
            .count();
        self.total_warnings = self
            .diagnostics
            .iter()
            .filter(|d| d.severity == LspSeverity::Warning)
            .count();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspPromptContext {
    pub total_errors: usize,
    pub total_warnings: usize,
    pub top_findings: Vec<String>,
}

impl LspPromptContext {
    pub fn is_empty(&self) -> bool {
        self.total_errors == 0 && self.total_warnings == 0
    }

    pub fn to_prompt_section(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut lines = vec![format!(
            "\n---\n**vil-lsp diagnostics** ({} errors, {} warnings):",
            self.total_errors, self.total_warnings
        )];
        for finding in &self.top_findings {
            lines.push(format!("- {finding}"));
        }
        lines.push("Prioritize fixing these before writing new code.".to_string());
        Some(lines.join("\n"))
    }
}

// ── Navigation types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocation {
    pub file_path: PathBuf,
    pub range: LspRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspHover {
    pub contents: String,
    pub range: Option<LspRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
    pub name: String,
    pub kind: u32, // LSP SymbolKind
    pub range: LspRange,
    pub detail: Option<String>,
}
