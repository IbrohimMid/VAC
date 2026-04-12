//! Per-run execution profiles for VAC.
//!
//! Profiles overlay behavior on top of base config:
//!   strict-vil    — planner gate hard, validator blocks, SHM required, no fallback
//!   migration     — planner required, validator warns, SHM optional
//!   exploration   — planner recommended, validator warns, fallback allowed
//!   spec-hardening — same as strict-vil but with extra validator strictness

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileName {
    #[default]
    Default,
    StrictVil,
    Migration,
    Exploration,
    SpecHardening,
    Custom(String),
}

impl std::fmt::Display for ProfileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::StrictVil => write!(f, "strict-vil"),
            Self::Migration => write!(f, "migration"),
            Self::Exploration => write!(f, "exploration"),
            Self::SpecHardening => write!(f, "spec-hardening"),
            Self::Custom(s) => write!(f, "{s}"),
        }
    }
}

impl ProfileName {
    pub fn from_str(s: &str) -> Self {
        match s {
            "strict-vil" | "strict_vil" => Self::StrictVil,
            "migration" => Self::Migration,
            "exploration" => Self::Exploration,
            "spec-hardening" | "spec_hardening" => Self::SpecHardening,
            "default" | "" => Self::Default,
            other => Self::Custom(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileOverride {
    pub name: ProfileName,
    /// Whether planner gate is enforced (fail-closed on parse/knowledge failure)
    pub planner_gate: bool,
    /// Whether validator blocks on semantic failures (vs warn-only)
    pub validator_blocks: bool,
    /// Whether SHM is required for tool execution
    pub require_shm: bool,
    /// Whether knowledge corpus must be authoritative (fail if bootstrap)
    pub require_authoritative_knowledge: bool,
    /// Whether planner fallback (raw output) is allowed when parse fails
    pub allow_fallback_plan: bool,
}

impl ProfileOverride {
    pub fn resolve(name: &ProfileName) -> Self {
        match name {
            ProfileName::StrictVil | ProfileName::SpecHardening => Self {
                name: name.clone(),
                planner_gate: true,
                validator_blocks: true,
                require_shm: true,
                require_authoritative_knowledge: true,
                allow_fallback_plan: false,
            },
            ProfileName::Migration => Self {
                name: name.clone(),
                planner_gate: true,
                validator_blocks: false,
                require_shm: false,
                require_authoritative_knowledge: false,
                allow_fallback_plan: true,
            },
            ProfileName::Exploration => Self {
                name: name.clone(),
                planner_gate: false,
                validator_blocks: false,
                require_shm: false,
                require_authoritative_knowledge: false,
                allow_fallback_plan: true,
            },
            ProfileName::Default | ProfileName::Custom(_) => Self {
                name: name.clone(),
                planner_gate: false,
                validator_blocks: false,
                require_shm: false,
                require_authoritative_knowledge: false,
                allow_fallback_plan: true,
            },
        }
    }
}
