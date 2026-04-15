//! Per-run execution profiles for VAC.

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
    pub fn parse(s: &str) -> Self {
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

impl std::str::FromStr for ProfileName {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileOverride {
    pub name: ProfileName,
    pub planner_gate: bool,
    pub validator_blocks: bool,
    pub require_shm: bool,
    pub require_authoritative_knowledge: bool,
    pub allow_fallback_plan: bool,
    pub subagents_enabled: bool,
    /// "ephemeral" | "persistent"
    pub sandbox_mode: String,
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
                subagents_enabled: true,
                sandbox_mode: "ephemeral".to_string(),
            },
            ProfileName::Migration => Self {
                name: name.clone(),
                planner_gate: true,
                validator_blocks: false,
                require_shm: false,
                require_authoritative_knowledge: false,
                allow_fallback_plan: true,
                subagents_enabled: true,
                sandbox_mode: "ephemeral".to_string(),
            },
            ProfileName::Exploration => Self {
                name: name.clone(),
                planner_gate: false,
                validator_blocks: false,
                require_shm: false,
                require_authoritative_knowledge: false,
                allow_fallback_plan: true,
                subagents_enabled: true,
                sandbox_mode: "persistent".to_string(),
            },
            ProfileName::Default | ProfileName::Custom(_) => Self {
                name: name.clone(),
                planner_gate: false,
                validator_blocks: false,
                require_shm: false,
                require_authoritative_knowledge: false,
                allow_fallback_plan: true,
                subagents_enabled: false,
                sandbox_mode: "ephemeral".to_string(),
            },
        }
    }
}
