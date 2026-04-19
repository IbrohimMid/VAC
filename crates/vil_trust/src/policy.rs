//! Policy engine for tool gating and permission enforcement.
//!
//! The [`PolicyEngine`] evaluates a [`PolicyRequest`] against (in order):
//! 1. explicit per-tool overrides,
//! 2. priority-sorted [`PolicyRule`]s,
//! 3. the configured [`DefaultPolicy`].

use crate::error::{TrustError, TrustResult};
use crate::zones::{RiskLevel, TrustZone};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// Evaluates policy rules to produce a [`PolicyDecision`] for a given request.
pub struct PolicyEngine {
    default_policy: DefaultPolicy,
    rules: Vec<PolicyRule>,
    tool_overrides: HashMap<String, PolicyDecision>,
}

/// What to do with a request that doesn't match any rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefaultPolicy {
    /// Fall through to Allow — permissive default for trusted environments.
    Allow,
    /// Fall through to Deny — secure default; callers must opt in via rules.
    Deny,
}

/// A named, prioritized rule that matches a [`PolicyRequest`] via its
/// [`PolicyCondition`] and produces a [`PolicyDecision`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Human-readable name used in logs.
    pub name: String,
    /// Condition that must match for this rule to fire.
    pub condition: PolicyCondition,
    /// Decision to return when the rule matches.
    pub decision: PolicyDecision,
    /// Higher values evaluate first.
    pub priority: i32,
}

/// Match condition for a [`PolicyRule`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyCondition {
    /// Match when the request's tool name equals this string.
    ToolName(String),
    /// Match when the request's risk level equals this variant.
    RiskLevel(RiskLevel),
    /// Match when the agent is in this trust zone.
    TrustZone(TrustZone),
    /// Match when the agent role equals this string.
    AgentRole(String),
    /// Always matches.
    Always,
}

/// Outcome of evaluating a [`PolicyRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Allow the action without further gating.
    Allow,
    /// Block the action.
    Deny,
    /// Surface an approval prompt to a human.
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
    /// Creates a new engine with the given fall-through behaviour and no rules.
    pub fn new(default: DefaultPolicy) -> Self {
        Self {
            default_policy: default,
            rules: Vec::new(),
            tool_overrides: HashMap::new(),
        }
    }

    /// Registers a rule. Rules are kept sorted by descending priority.
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Sets a per-tool override that short-circuits rule evaluation for the
    /// given tool name.
    pub fn set_tool_override(&mut self, tool_name: &str, decision: PolicyDecision) {
        self.tool_overrides.insert(tool_name.to_string(), decision);
    }

    /// Evaluates a request and returns the resulting decision without
    /// side-effects (other than log emission).
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

    /// Like [`PolicyEngine::evaluate`] but converts Deny to `PermissionDenied`
    /// and RequireApproval to `RequiresApproval`.
    pub fn enforce(&self, request: &PolicyRequest) -> TrustResult<()> {
        match self.evaluate(request)? {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Deny => Err(TrustError::PermissionDenied {
                action: request.tool_name.clone(),
                required: format!("explicit allow for '{}'", request.tool_name),
            }),
            PolicyDecision::RequireApproval => {
                warn!(tool = %request.tool_name, agent = %request.agent_id, "Action requires human approval");
                Err(TrustError::RequiresApproval {
                    action: request.tool_name.clone(),
                    reason: "human approval".to_string(),
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

/// Input to [`PolicyEngine::evaluate`] describing the action and the agent.
#[derive(Debug, Clone)]
pub struct PolicyRequest {
    /// Name of the tool being invoked (e.g. `"file_write"`).
    pub tool_name: String,
    /// Stable identifier of the agent making the request.
    pub agent_id: String,
    /// Agent role/profile (e.g. `"coder"`, `"reviewer"`).
    pub agent_role: String,
    /// Trust zone the agent currently runs in.
    pub agent_zone: TrustZone,
    /// Inherent risk of the action, independent of agent identity.
    pub risk_level: RiskLevel,
    /// Short human-readable summary of the arguments for logging / audit.
    pub arguments_summary: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_request(tool_name: &str, risk_level: RiskLevel) -> PolicyRequest {
        PolicyRequest {
            tool_name: tool_name.to_string(),
            agent_id: "test-agent".to_string(),
            agent_role: "coder".to_string(),
            agent_zone: TrustZone::Trusted,
            risk_level,
            arguments_summary: "test".to_string(),
        }
    }

    #[test]
    fn enforce_allow_returns_ok() {
        let engine = PolicyEngine::default();
        let req = make_request("file_read", RiskLevel::Safe);
        assert!(engine.enforce(&req).is_ok());
    }

    #[test]
    fn enforce_deny_returns_permission_denied_variant() {
        let mut engine = PolicyEngine::new(DefaultPolicy::Deny);
        engine.add_rule(PolicyRule {
            name: "deny_all".into(),
            condition: PolicyCondition::Always,
            decision: PolicyDecision::Deny,
            priority: 1,
        });
        let req = make_request("bash", RiskLevel::Dangerous);
        let err = engine.enforce(&req).unwrap_err();
        assert!(matches!(err, TrustError::PermissionDenied { .. }));
    }

    #[test]
    fn enforce_require_approval_returns_requires_approval_variant() {
        let engine = PolicyEngine::default();
        let req = make_request("file_write", RiskLevel::NeedsApproval);
        let err = engine.enforce(&req).unwrap_err();
        assert!(matches!(err, TrustError::RequiresApproval { .. }));
        if let TrustError::RequiresApproval { action, reason } = err {
            assert_eq!(action, "file_write");
            assert_eq!(reason, "human approval");
        }
    }
}
