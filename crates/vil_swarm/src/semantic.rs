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
            - **Rationale:** {}\n",
            self.kind,
            self.semantic_roles.join(", "),
            self.lanes.join(", "),
            self.zero_copy_expected,
            self.generated_plumbing_expected,
            self.forbidden_constructs.join(", "),
            self.required_patterns.join(", "),
            self.knowledge_refs.join(", "),
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
