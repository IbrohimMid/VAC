//! Policy engine for tool gating and permission enforcement.

use crate::error::{TrustError, TrustResult};
use crate::zones::{RiskLevel, TrustZone};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

pub struct PolicyEngine {
    default_policy: DefaultPolicy,
    rules: Vec<PolicyRule>,
    tool_overrides: HashMap<String, PolicyDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefaultPolicy {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub name: String,
    pub condition: PolicyCondition,
    pub decision: PolicyDecision,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyCondition {
    ToolName(String),
    RiskLevel(RiskLevel),
    TrustZone(TrustZone),
    AgentRole(String),
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny,
    RequireApproval,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        let mut engine = Self::new(DefaultPolicy::Deny);

        // Allow all read tools by default
        engine.add_rule(PolicyRule {
            name: "allow_read_tools".into(),
            condition: PolicyCondition::RiskLevel(RiskLevel::Safe),
            decision: PolicyDecision::Allow,
            priority: 10,
        });

        // Require approval for write/modify tools
        engine.add_rule(PolicyRule {
            name: "write_tools_require_approval".into(),
            condition: PolicyCondition::RiskLevel(RiskLevel::NeedsApproval),
            decision: PolicyDecision::RequireApproval,
            priority: 5,
        });

        engine.add_rule(PolicyRule {
            name: "write_tools_require_approval_high".into(),
            condition: PolicyCondition::RiskLevel(RiskLevel::Dangerous),
            decision: PolicyDecision::RequireApproval,
            priority: 5,
        });

        engine
    }
}

impl PolicyEngine {
    pub fn new(default: DefaultPolicy) -> Self {
        Self {
            default_policy: default,
            rules: Vec::new(),
            tool_overrides: HashMap::new(),
        }
    }

    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    pub fn set_tool_override(&mut self, tool_name: &str, decision: PolicyDecision) {
        self.tool_overrides.insert(tool_name.to_string(), decision);
    }

    pub fn evaluate(&self, request: &PolicyRequest) -> TrustResult<PolicyDecision> {
        if let Some(decision) = self.tool_overrides.get(&request.tool_name) {
            info!(tool = %request.tool_name, decision = ?decision, "Policy override applied");
            return Ok(*decision);
        }

        for rule in &self.rules {
            if rule.matches(request) {
                info!(rule = %rule.name, decision = ?rule.decision, "Policy rule matched");
                return Ok(rule.decision);
            }
        }

        let decision = match self.default_policy {
            DefaultPolicy::Allow => PolicyDecision::Allow,
            DefaultPolicy::Deny => PolicyDecision::Deny,
        };
        Ok(decision)
    }

    pub fn enforce(&self, request: &PolicyRequest) -> TrustResult<()> {
        match self.evaluate(request)? {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Deny => Err(TrustError::PermissionDenied {
                action: request.tool_name.clone(),
                required: format!("explicit allow for '{}'", request.tool_name),
            }),
            PolicyDecision::RequireApproval => {
                warn!(tool = %request.tool_name, "Action requires human approval");
                Err(TrustError::PermissionDenied {
                    action: request.tool_name.clone(),
                    required: "human approval".to_string(),
                })
            }
        }
    }
}

impl PolicyRule {
    fn matches(&self, request: &PolicyRequest) -> bool {
        match &self.condition {
            PolicyCondition::ToolName(name) => &request.tool_name == name,
            PolicyCondition::RiskLevel(level) => &request.risk_level == level,
            PolicyCondition::TrustZone(zone) => &request.agent_zone == zone,
            PolicyCondition::AgentRole(role) => &request.agent_role == role,
            PolicyCondition::Always => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyRequest {
    pub tool_name: String,
    pub agent_id: String,
    pub agent_role: String,
    pub agent_zone: TrustZone,
    pub risk_level: RiskLevel,
    pub arguments_summary: String,
}
