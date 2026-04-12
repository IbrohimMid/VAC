//! Semantic planning layer for VIL-native tasks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskSemanticKind {
    VxApp,
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
}

impl SemanticPlan {
    pub fn is_vil_specific(&self) -> bool {
        self.kind != TaskSemanticKind::GenericRust && self.kind != TaskSemanticKind::Unknown
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
            - **Rationale:** {}\n",
            self.kind,
            self.semantic_roles.join(", "),
            self.lanes.join(", "),
            self.zero_copy_expected,
            self.generated_plumbing_expected,
            self.forbidden_constructs.join(", "),
            self.required_patterns.join(", "),
            self.rationale
        )
    }
}
