//! Semantic planning layer for VIL-native tasks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskSemanticKind {
    /// Canonical VIL server kind. Accepts legacy alias "VxApp" on deserialization.
    #[serde(alias = "VxApp")]
    VilServer,
    SdkPipeline,
    Plugin,
    Wasm,
    Sidecar,
    Connector,
    SemanticMessageLayer,
    GenericRust,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticPlan {
    pub kind: TaskSemanticKind,
    pub semantic_roles: Vec<String>,
    pub lanes: Vec<String>,
    pub zero_copy_expected: bool,
    pub generated_plumbing_expected: bool,
    pub forbidden_constructs: Vec<String>,
    pub required_patterns: Vec<String>,
    pub rationale: String,
    /// Knowledge base entries consulted during planning (required in strict mode)
    #[serde(default)]
    pub knowledge_refs: Vec<String>,
    /// Task graph parent ID (if this is a subtask)
    #[serde(default)]
    pub parent_task_id: Option<String>,
    /// Dependencies (other task IDs that must complete before this one)
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Retry policy
    #[serde(default)]
    pub max_retries: Option<u32>,
    /// Budget tokens
    #[serde(default)]
    pub budget_tokens: Option<u64>,
    /// Optional worktree path for this subtask
    #[serde(default)]
    pub worktree_path: Option<String>,
    /// Approval policy profile
    #[serde(default)]
    pub approval_policy: Option<String>,
}

impl SemanticPlan {
    pub fn is_vil_specific(&self) -> bool {
        self.kind != TaskSemanticKind::GenericRust && self.kind != TaskSemanticKind::Unknown
    }

    /// In strict mode, a VIL-specific plan must have at least one knowledge_ref.
    pub fn knowledge_gate_passed(&self) -> bool {
        if self.is_vil_specific() {
            !self.knowledge_refs.is_empty()
        } else {
            true // GenericRust/Unknown don't require knowledge lookup
        }
    }

    /// Check that the plan's string fields don't contain legacy VIL aliases.
    /// Returns (passed, warnings) — does not affect knowledge_gate_passed().
    pub fn canonical_terms_gate(&self) -> (bool, Vec<String>) {
        use vil_knowledge::canonical::{ValidationMode, check_terms};
        let content = format!(
            "{:?} {} {}",
            self.kind,
            self.required_patterns.join(" "),
            self.rationale
        );
        let result = check_terms(&content, ValidationMode::Strict);
        (result.passed, result.errors)
    }

    pub fn to_markdown(&self) -> String {
        format!(
            "### VIL Semantic Plan\n\
            - **Kind:** {:?}\n\
            - **Semantic Roles:** {}\n\
            - **Lanes:** {}\n\
            - **Zero-Copy Expected:** {}\n\
            - **Generated Plumbing Expected:** {}\n\
            - **Forbidden Constructs:** {}\n\
            - **Required Patterns:** {}\n\
            - **Knowledge Refs:** {}\n\
            - **Dependencies:** {}\n\
            - **Approval Policy:** {}\n\
            - **Rationale:** {}\n",
            self.kind,
            self.semantic_roles.join(", "),
            self.lanes.join(", "),
            self.zero_copy_expected,
            self.generated_plumbing_expected,
            self.forbidden_constructs.join(", "),
            self.required_patterns.join(", "),
            self.knowledge_refs.join(", "),
            self.dependencies.join(", "),
            self.approval_policy.as_deref().unwrap_or("default"),
            self.rationale
        )
    }
}

/// Result of the planner gate check.
#[derive(Debug)]
pub enum PlannerGateResult {
    /// Plan parsed and passed all gates.
    Passed(SemanticPlan),
    /// Plan parsed but knowledge gate failed (VIL task without knowledge lookup).
    KnowledgeGateFailed(SemanticPlan),
    /// Plan could not be parsed from planner output.
    ParseFailed(String),
}

/// Parse and gate-check a planner output string.
pub fn evaluate_planner_output(output: &str) -> PlannerGateResult {
    let plan = extract_plan_json(output);

    match plan {
        None => PlannerGateResult::ParseFailed(output.to_string()),
        Some(plan) => {
            // Log canonical term warnings (non-blocking)
            let (_, warnings) = plan.canonical_terms_gate();
            for w in &warnings {
                tracing::warn!(canonical_term_issue = %w, "Planner output contains legacy VIL alias");
            }
            if plan.knowledge_gate_passed() {
                PlannerGateResult::Passed(plan)
            } else {
                PlannerGateResult::KnowledgeGateFailed(plan)
            }
        }
    }
}

fn extract_plan_json(output: &str) -> Option<SemanticPlan> {
    // Try ```json ... ``` block first
    if let Some(start) = output.find("```json") {
        let json_start = start + 7;
        if let Some(end) = output[json_start..].find("```") {
            let json_str = &output[json_start..json_start + end];
            if let Ok(plan) = serde_json::from_str::<SemanticPlan>(json_str) {
                return Some(plan);
            }
        }
    }
    // Try bare JSON
    serde_json::from_str::<SemanticPlan>(output).ok()
}
