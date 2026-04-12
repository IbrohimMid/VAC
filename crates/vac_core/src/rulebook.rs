//! Rulebook overlay — local/team constraints layered on top of llm_knowledge.
//!
//! Precedence (highest to lowest):
//!   1. llm_knowledge/ corpus (authoritative VIL semantics)
//!   2. Rulebook (.vac/rules.toml or .vac/rules/*.toml)
//!   3. Local hints (inline task context)
//!
//! Rulebooks MUST NOT override VIL semantic contracts.
//! They are for: team conventions, repo constraints, org policies, acceptance gates.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rulebook {
    /// Human-readable name for this rulebook
    pub name: Option<String>,
    /// Coding conventions specific to this repo/team
    #[serde(default)]
    pub conventions: Vec<Rule>,
    /// Acceptance gates — conditions that must pass before task is considered done
    #[serde(default)]
    pub acceptance_gates: Vec<Rule>,
    /// Org-level policies (e.g. "never commit secrets", "always add tests")
    #[serde(default)]
    pub policies: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub description: String,
    /// Optional: pattern to detect violation (regex or keyword)
    pub detect: Option<String>,
    /// Severity: "warn" | "block"
    #[serde(default = "default_severity")]
    pub severity: String,
}

fn default_severity() -> String {
    "warn".to_string()
}

impl Rulebook {
    /// Load rulebook from .vac/rules.toml, or empty if not present.
    pub fn load(project_root: &Path) -> Self {
        let path = project_root.join(".vac/rules.toml");
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<Self>(&s).ok())
        {
            Some(rb) => {
                tracing::info!(path = %path.display(), "Rulebook loaded");
                rb
            }
            None => {
                tracing::debug!("No rulebook found or parse failed, using empty");
                Self::default()
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.conventions.is_empty()
            && self.acceptance_gates.is_empty()
            && self.policies.is_empty()
    }

    /// Format rulebook as a prompt overlay section.
    /// This is appended AFTER the authoritative VIL knowledge context, never before.
    pub fn to_prompt_overlay(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        let mut lines = vec![
            "\n---\n**Team/Repo Rules (overlay — VIL semantics take precedence):**".to_string(),
        ];

        if !self.conventions.is_empty() {
            lines.push("Conventions:".to_string());
            for r in &self.conventions {
                lines.push(format!("- [{}] {}", r.id, r.description));
            }
        }
        if !self.policies.is_empty() {
            lines.push("Policies:".to_string());
            for r in &self.policies {
                lines.push(format!("- [{}] {}", r.id, r.description));
            }
        }
        if !self.acceptance_gates.is_empty() {
            lines.push("Acceptance gates:".to_string());
            for r in &self.acceptance_gates {
                let marker = if r.severity == "block" { "🔴" } else { "⚠️" };
                lines.push(format!("- {} [{}] {}", marker, r.id, r.description));
            }
        }

        Some(lines.join("\n"))
    }
}
