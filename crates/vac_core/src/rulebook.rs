//! Rulebook engine — multi-rulebook governance with strict precedence.
//!
//! Precedence (highest to lowest):
//!   1. llm_knowledge/ corpus  (authoritative VIL semantics — never overridable)
//!   2. vil_validate / vil-lsp  (semantic enforcement)
//!   3. Rulebook constraints    (team/repo/org overlay)
//!   4. Session/user hints      (ephemeral context)
//!
//! Rulebooks MUST NOT override VIL semantic contracts.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ── Core types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleScope {
    Workspace,
    Repo,
    Profile(String),
    Team,
    Archetype(String), // "server" | "pipeline" | "plugin"
    Global,
}

impl RuleScope {
    pub fn matches_archetype(&self, archetype: Option<&str>) -> bool {
        match self {
            Self::Archetype(a) => archetype
                .map(|arch| arch.contains(a.as_str()))
                .unwrap_or(false),
            _ => true, // non-archetype scopes always match
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConstraint {
    pub id: String,
    pub description: String,
    /// Optional regex/keyword to detect violation in code
    pub detect: Option<String>,
    /// "warn" | "block"
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(default)]
    pub scope: Option<RuleScope>,
}

fn default_severity() -> String {
    "warn".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rulebook {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub scope: Option<RuleScope>,
    /// Higher priority wins on conflict (default 0)
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub constraints: Vec<RuleConstraint>,
    /// Deprecated single-file compat fields
    #[serde(default)]
    pub conventions: Vec<RuleConstraint>,
    #[serde(default)]
    pub acceptance_gates: Vec<RuleConstraint>,
    #[serde(default)]
    pub policies: Vec<RuleConstraint>,
}

impl Rulebook {
    /// Collect all constraints from all sections.
    pub fn all_constraints(&self) -> Vec<&RuleConstraint> {
        self.constraints
            .iter()
            .chain(self.conventions.iter())
            .chain(self.acceptance_gates.iter())
            .chain(self.policies.iter())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.constraints.is_empty()
            && self.conventions.is_empty()
            && self.acceptance_gates.is_empty()
            && self.policies.is_empty()
    }
}

// ── Loader ────────────────────────────────────────────────────────────────────

pub struct RulebookLoader;

impl RulebookLoader {
    /// Load all rulebooks from configured paths.
    /// Paths are checked in order; later paths have lower priority unless priority field overrides.
    pub fn load_all(project_root: &Path, extra_paths: &[PathBuf]) -> Vec<Rulebook> {
        let mut books = Vec::new();

        // 1. Single-file compat: .vac/rules.toml
        let single = project_root.join(".vac/rules.toml");
        if single.exists() {
            if let Some(rb) = Self::load_file(&single, "default") {
                books.push(rb);
            }
        }

        // 2. Multi-file: .vac/rulebooks/*.toml
        let multi_dir = project_root.join(".vac/rulebooks");
        books.extend(Self::load_dir(&multi_dir));

        // 3. Extra paths from config
        for path in extra_paths {
            let expanded = expand_tilde(path);
            if expanded.is_dir() {
                books.extend(Self::load_dir(&expanded));
            } else if expanded.exists() {
                if let Some(rb) = Self::load_file(&expanded, &expanded.display().to_string()) {
                    books.push(rb);
                }
            }
        }

        books
    }

    fn load_dir(dir: &Path) -> Vec<Rulebook> {
        if !dir.is_dir() {
            return vec![];
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return vec![];
        };
        entries
            .flatten()
            .filter(|e| e.path().extension().map(|x| x == "toml").unwrap_or(false))
            .filter_map(|e| {
                let id = e
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                Self::load_file(&e.path(), &id)
            })
            .collect()
    }

    fn load_file(path: &Path, default_id: &str) -> Option<Rulebook> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut rb: Rulebook = toml::from_str(&content).ok()?;
        if rb.id.is_empty() {
            rb.id = default_id.to_string();
        }
        tracing::debug!(path = %path.display(), id = %rb.id, "Rulebook loaded");
        Some(rb)
    }
}

// ── Merger ────────────────────────────────────────────────────────────────────

pub struct RulebookMerger;

impl RulebookMerger {
    /// Merge multiple rulebooks, deduplicating by constraint id (higher priority wins).
    pub fn merge(mut books: Vec<Rulebook>) -> Vec<RuleConstraint> {
        // Sort by priority descending
        books.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut merged = Vec::new();

        for book in &books {
            for constraint in book.all_constraints() {
                if seen_ids.insert(constraint.id.clone()) {
                    merged.push(constraint.clone());
                }
            }
        }

        merged
    }
}

// ── Validator ─────────────────────────────────────────────────────────────────

/// VIL core rule ids that must never be overridden by rulebooks.
const VIL_CORE_RULE_IDS: &[&str] = &[
    "vil-shm-slice",
    "vil-service-ctx",
    "vil-response",
    "vil-handler",
    "vil-state-macro",
    "vil-event-macro",
    "vil-fault-macro",
    "vil-decision-macro",
    "vil-no-json-extractor",
    "vil-no-extension-extractor",
];

#[derive(Debug)]
pub struct RulebookValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl RulebookValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate_rulebooks(books: &[Rulebook]) -> RulebookValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut all_ids: HashMap<String, String> = HashMap::new(); // id -> book_id

    for book in books {
        for constraint in book.all_constraints() {
            // Check VIL core override
            if VIL_CORE_RULE_IDS.contains(&constraint.id.as_str()) {
                errors.push(format!(
                    "Rulebook '{}': constraint '{}' attempts to override VIL core rule — forbidden",
                    book.id, constraint.id
                ));
            }
            // Check duplicate ids across books
            if let Some(existing_book) = all_ids.get(&constraint.id) {
                warnings.push(format!(
                    "Duplicate rule id '{}' in books '{}' and '{}' — higher priority wins",
                    constraint.id, existing_book, book.id
                ));
            } else {
                all_ids.insert(constraint.id.clone(), book.id.clone());
            }
        }
    }

    RulebookValidationResult { errors, warnings }
}

// ── Resolved context ──────────────────────────────────────────────────────────

pub struct ResolvedRuleContext {
    pub constraints: Vec<RuleConstraint>,
    pub archetype: Option<String>,
}

impl ResolvedRuleContext {
    pub fn build(books: Vec<Rulebook>, archetype: Option<&str>) -> Self {
        let all = RulebookMerger::merge(books);
        let constraints = all
            .into_iter()
            .filter(|c| {
                c.scope
                    .as_ref()
                    .map(|s| s.matches_archetype(archetype))
                    .unwrap_or(true)
            })
            .collect();

        Self {
            constraints,
            archetype: archetype.map(String::from),
        }
    }

    pub fn to_prompt_overlay(&self) -> Option<String> {
        if self.constraints.is_empty() {
            return None;
        }

        let mut lines = vec![
            "\n---\n**Team/Repo Rules (overlay — VIL semantics take precedence):**".to_string(),
        ];

        for c in &self.constraints {
            let marker = if c.severity == "block" {
                "🔴"
            } else {
                "⚠️"
            };
            lines.push(format!("{} [{}] {}", marker, c.id, c.description));
        }

        Some(lines.join("\n"))
    }

    pub fn blocking_constraints(&self) -> Vec<&RuleConstraint> {
        self.constraints
            .iter()
            .filter(|c| c.severity == "block")
            .collect()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path.to_path_buf()
}

// ── Backward compat ───────────────────────────────────────────────────────────

/// Load single rulebook (backward compat with Phase P2.3 API).
pub fn load_single(project_root: &Path) -> Option<ResolvedRuleContext> {
    let books = RulebookLoader::load_all(project_root, &[]);
    if books.is_empty() {
        return None;
    }
    Some(ResolvedRuleContext::build(books, None))
}

// ── F6.3 — Rulebook URI resolver ──────────────────────────────────────────────

/// Parsed `vac://rulebook/<id>` reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulebookUri {
    /// Rulebook id as referenced (`<id>`).
    pub id: String,
    /// Optional `#section` fragment (e.g. a constraint key).
    pub section: Option<String>,
}

/// Errors from [`parse_rulebook_uri`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RulebookUriError {
    #[error("uri must start with 'vac://rulebook/'")]
    WrongScheme,
    #[error("rulebook id is empty")]
    EmptyId,
    #[error("rulebook id contains illegal character: {0:?}")]
    IllegalIdChar(char),
}

const RULEBOOK_URI_PREFIX: &str = "vac://rulebook/";

/// Parse a `vac://rulebook/<id>[#section]` URI. The id must match
/// `[A-Za-z0-9._-]+`; fragments are arbitrary non-empty strings.
pub fn parse_rulebook_uri(uri: &str) -> Result<RulebookUri, RulebookUriError> {
    let rest = uri
        .strip_prefix(RULEBOOK_URI_PREFIX)
        .ok_or(RulebookUriError::WrongScheme)?;
    let (id_part, section) = match rest.split_once('#') {
        Some((id, frag)) if !frag.is_empty() => (id, Some(frag.to_string())),
        Some((id, _)) => (id, None),
        None => (rest, None),
    };
    if id_part.is_empty() {
        return Err(RulebookUriError::EmptyId);
    }
    for ch in id_part.chars() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')) {
            return Err(RulebookUriError::IllegalIdChar(ch));
        }
    }
    Ok(RulebookUri {
        id: id_part.to_string(),
        section,
    })
}

/// Resolve a `vac://rulebook/<id>` URI against a pool of loaded
/// rulebooks. Returns `None` if no id matches. Section fragment is
/// reported alongside the rulebook so callers can drill in.
pub fn resolve_rulebook_uri<'a>(
    uri: &str,
    pool: &'a [Rulebook],
) -> Result<Option<(&'a Rulebook, Option<String>)>, RulebookUriError> {
    let parsed = parse_rulebook_uri(uri)?;
    Ok(pool
        .iter()
        .find(|rb| rb.id == parsed.id)
        .map(|rb| (rb, parsed.section)))
}

#[cfg(test)]
mod uri_tests {
    use super::*;

    fn rb(id: &str) -> Rulebook {
        Rulebook {
            id: id.into(),
            name: None,
            scope: None,
            priority: 0,
            constraints: vec![],
            conventions: vec![],
            acceptance_gates: vec![],
            policies: vec![],
        }
    }

    #[test]
    fn parses_plain_id() {
        let p = parse_rulebook_uri("vac://rulebook/team.defaults").unwrap();
        assert_eq!(p.id, "team.defaults");
        assert!(p.section.is_none());
    }

    #[test]
    fn parses_id_with_section() {
        let p = parse_rulebook_uri("vac://rulebook/core#tests-must-pass").unwrap();
        assert_eq!(p.id, "core");
        assert_eq!(p.section.as_deref(), Some("tests-must-pass"));
    }

    #[test]
    fn rejects_wrong_scheme() {
        let e = parse_rulebook_uri("http://rulebook/x").unwrap_err();
        assert!(matches!(e, RulebookUriError::WrongScheme));
    }

    #[test]
    fn rejects_empty_id() {
        let e = parse_rulebook_uri("vac://rulebook/").unwrap_err();
        assert!(matches!(e, RulebookUriError::EmptyId));
    }

    #[test]
    fn rejects_illegal_id_char() {
        let e = parse_rulebook_uri("vac://rulebook/bad id").unwrap_err();
        assert!(matches!(e, RulebookUriError::IllegalIdChar(' ')));
    }

    #[test]
    fn resolve_finds_matching_rulebook() {
        let pool = vec![rb("core"), rb("team")];
        let (hit, section) = resolve_rulebook_uri("vac://rulebook/team#sop", &pool)
            .unwrap()
            .unwrap();
        assert_eq!(hit.id, "team");
        assert_eq!(section.as_deref(), Some("sop"));
    }

    #[test]
    fn resolve_missing_returns_none() {
        let pool = vec![rb("core")];
        assert!(resolve_rulebook_uri("vac://rulebook/absent", &pool)
            .unwrap()
            .is_none());
    }
}
