//! Runtime and VIL engine state types.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Runtime / agent-scheduler domain state. Accessed via `app_state.execution.runtime`.
#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    pub jobs: Vec<vac_runtime::Job>,
    pub selected_idx: usize,
    pub filter: String,
    pub detail_scroll: usize,
    pub snapshot: Option<vac_runtime::AutopilotStateFile>,
    pub agent_tasks: Vec<vac_runtime::AgentTask>,
    pub agent_selected: usize,
    pub agent_detail_scroll: usize,
    pub agent_snapshot: Option<vac_runtime::AgentSchedulerStateFile>,
    pub task_projection: Option<vac_core::engine::TaskGraphProjection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityKind {
    Status,
    Tool,
    Approval,
    Review,
    Session,
    Error,
    Mcp,
    Isolation,
    Shell,
}

#[derive(Debug, Clone)]
pub struct ActivityItem {
    pub at: DateTime<Utc>,
    pub kind: ActivityKind,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct VilLogEntry {
    pub at: DateTime<Utc>,
    pub message: String,
}

/// Issue severity level for VIL validation issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum VilSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

impl VilSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }
}

/// Issue kind inferred from free-text validation issue strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum VilIssueKind {
    Semantic,
    ZeroCopy,
    Plumbing,
    IrDrift,
    CanonicalTerm,
    Other,
}

impl VilIssueKind {
    /// Keyword-based heuristic classifier for free-text validation issues.
    pub fn classify(text: &str) -> Self {
        let lower = text.to_ascii_lowercase();
        if lower.contains("zero-copy")
            || lower.contains("zerocopy")
            || lower.contains("zero_copy")
            || lower.contains("owned-bytes")
        {
            Self::ZeroCopy
        } else if lower.contains("plumbing")
            || lower.contains("manually implements")
            || lower.contains("remove plumbing")
        {
            Self::Plumbing
        } else if lower.contains("ir drift")
            || lower.contains("ir-drift")
            || lower.contains("ir metadata drift")
        {
            Self::IrDrift
        } else if lower.contains("canonical term") || lower.contains("canonical-term") {
            Self::CanonicalTerm
        } else if lower.contains("semantic")
            || lower.contains("vil role macro")
            || lower.contains("#[vil_state]")
            || lower.contains("#[vil_event]")
            || lower.contains("#[vil_fault]")
            || lower.contains("#[vil_decision]")
        {
            Self::Semantic
        } else {
            Self::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Semantic => "Semantic",
            Self::ZeroCopy => "ZeroCopy",
            Self::Plumbing => "Plumbing",
            Self::IrDrift => "IrDrift",
            Self::CanonicalTerm => "Canonical",
            Self::Other => "Other",
        }
    }
}

/// Structured VIL validation issue.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VilIssue {
    pub id: String,
    pub kind: VilIssueKind,
    pub severity: VilSeverity,
    pub source: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub raw: Option<String>,
    pub repair_proposal: Option<String>,
    pub inferred: bool,
}

impl VilIssue {
    /// Create a VilIssue from a raw string, inferring kind and other fields.
    pub fn from_raw(raw: String) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        raw.hash(&mut hasher);
        let id = format!("{:08x}", hasher.finish());

        let kind = VilIssueKind::classify(&raw);
        let file = extract_file_hint(&raw);
        let message = shorten(&raw);

        let repair_proposal = match kind {
            VilIssueKind::Semantic => {
                Some("Apply #[vil_state] macro and derive VilMessage".to_string())
            }
            VilIssueKind::ZeroCopy => Some("Convert Vec<u8> to ShmSlice<u8>".to_string()),
            _ => None,
        };

        let severity = match kind {
            VilIssueKind::Semantic | VilIssueKind::ZeroCopy => VilSeverity::Error,
            VilIssueKind::Plumbing => VilSeverity::Warning,
            VilIssueKind::IrDrift => VilSeverity::Warning,
            VilIssueKind::CanonicalTerm => VilSeverity::Info,
            VilIssueKind::Other => VilSeverity::Info,
        };

        Self {
            id,
            kind,
            severity,
            source: "vil_validate".to_string(),
            message,
            file,
            line: None,
            column: None,
            raw: Some(raw),
            repair_proposal,
            inferred: true,
        }
    }
}

fn extract_file_hint(text: &str) -> Option<String> {
    for (i, c) in text.char_indices() {
        if c == '\'' {
            let rest = &text[i + 1..];
            if let Some(end) = rest.find('\'') {
                let name = &rest[..end];
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

pub fn shorten(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX: usize = 160;
    if collapsed.len() <= MAX {
        collapsed
    } else {
        let mut t = collapsed;
        t.truncate(MAX.saturating_sub(3));
        t.push('…');
        t
    }
}

#[derive(Debug, Clone, Default)]
pub struct VilStatusSnapshot {
    pub profile: Option<vac_core::detector::VilProjectProfile>,
    pub validation_score: f64,
    pub validation_issues: Vec<VilIssue>,
    pub active_rulebook: Option<String>,
    pub semantic_mode: bool,
    pub ir_generation_active: bool,
    pub ir_metadata_files: Vec<String>,
}

/// VIL-engine domain state. Accessed via `app_state.vil_domain.vil`.
#[derive(Debug, Clone, Default)]
pub struct VilState {
    pub status: VilStatusSnapshot,
    pub last_score: Option<f64>,
    pub score_history: Vec<f64>,
    pub event_log: VecDeque<VilLogEntry>,
    pub workbench_selected: usize,
    pub workbench_group_filter: Option<VilIssueKind>,
}
