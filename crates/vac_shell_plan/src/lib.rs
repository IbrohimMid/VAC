//! Slice 12 — plan-file parser/helper.
//!
//! Pure DTO + parser surface. The path always comes from
//! `VacPaths::plan_file()`; nothing here composes
//! `.stakpak/session/plan.md`. Reading and watching the file
//! is the host crate's job (`vac_shell_host_plan`).

// Slice 20.1 — `PlanMetadata` and `PlanStatus` moved to
// `vac_shell_contracts` so UI widgets stay on the canonical
// `ratatui + vac_shell_contracts` dep graph. Re-exported here so
// existing call sites (parser, host plan reader, tests) keep
// compiling without churn.
pub use vac_shell_contracts::{PlanMetadata, PlanStatus};

/// Extract the YAML front-matter block from a plan markdown file.
/// The donor permits two delimiters; we accept the standard
/// `---` fence. Returns `None` when no fence is present.
pub fn extract_front_matter(source: &str) -> Option<&str> {
    let s = source.strip_prefix("---")?;
    let s = s.strip_prefix('\n').unwrap_or(s);
    let end = s.find("\n---")?;
    Some(&s[..end])
}

#[derive(Debug, thiserror::Error)]
pub enum PlanParseError {
    #[error("plan file is missing the YAML front-matter fence")]
    MissingFrontMatter,
    #[error("plan front-matter is not valid YAML: {0}")]
    InvalidYaml(String),
}

/// Parse a plan markdown source into [`PlanMetadata`]. Slice 12
/// uses a tiny key:value parser rather than a full YAML library
/// dep — front-matter shape is fixed (`status`, `objective`,
/// `steps`, `blocked_on`).
pub fn parse_plan_front_matter(source: &str) -> Result<PlanMetadata, PlanParseError> {
    let front = extract_front_matter(source).ok_or(PlanParseError::MissingFrontMatter)?;
    let mut meta = PlanMetadata::default();
    let mut current_list: Option<&'static str> = None;
    for raw_line in front.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(item) = line.strip_prefix("- ") {
            let value = item.trim().trim_matches('"').to_string();
            match current_list {
                Some("steps") => meta.steps.push(value),
                Some("blocked_on") => meta.blocked_on.push(value),
                _ => {
                    return Err(PlanParseError::InvalidYaml(format!(
                        "list item without preceding key: {raw_line}"
                    )));
                }
            }
            continue;
        }
        let (key, rest) = match line.split_once(':') {
            Some(kv) => kv,
            None => {
                return Err(PlanParseError::InvalidYaml(format!(
                    "expected key: value, got {raw_line}"
                )));
            }
        };
        let key = key.trim();
        let value = rest.trim();
        match key {
            "status" => {
                meta.status = match value.trim_matches('"') {
                    "draft" => PlanStatus::Draft,
                    "active" => PlanStatus::Active,
                    "blocked" => PlanStatus::Blocked,
                    "done" => PlanStatus::Done,
                    "cancelled" | "canceled" => PlanStatus::Cancelled,
                    other => {
                        return Err(PlanParseError::InvalidYaml(format!(
                            "unknown status: {other}"
                        )));
                    }
                };
                current_list = None;
            }
            "objective" => {
                let v = value.trim_matches('"').to_string();
                meta.objective = if v.is_empty() { None } else { Some(v) };
                current_list = None;
            }
            "steps" => current_list = Some("steps"),
            "blocked_on" => current_list = Some("blocked_on"),
            other => {
                return Err(PlanParseError::InvalidYaml(format!("unknown key: {other}")));
            }
        }
    }
    Ok(meta)
}
