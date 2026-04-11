use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::error::ToolError;
use crate::registry::{ToolContext, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfigStub {
    pub default_policy: String,
    pub allow: HashMap<String, bool>,
    pub deny: HashMap<String, bool>,
}

impl Default for ToolConfigStub {
    fn default() -> Self {
        Self {
            default_policy: "deny".to_string(),
            allow: HashMap::new(),
            deny: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PolicyDecision {
    Allow,
    Deny(String),
    NeedsApproval(String),
}

#[async_trait]
pub trait PolicyEngine: Send + Sync {
    async fn decide(&self, tool_name: &str, context: &ToolContext) -> PolicyDecision;
}

pub struct DefaultPolicyEngine {
    config: Arc<ToolConfigStub>,
}

impl DefaultPolicyEngine {
    pub fn new(config: ToolConfigStub) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

#[async_trait]
impl PolicyEngine for DefaultPolicyEngine {
    async fn decide(&self, tool_name: &str, _context: &ToolContext) -> PolicyDecision {
        if let Some(&allowed) = self.config.allow.get(tool_name) {
            if allowed {
                debug!("Tool {} explicitly allowed by policy", tool_name);
                return PolicyDecision::Allow;
            }
        }

        if let Some(&denied) = self.config.deny.get(tool_name) {
            if denied {
                warn!("Tool {} explicitly denied by policy", tool_name);
                return PolicyDecision::Deny(format!("Tool {} is explicitly denied", tool_name));
            }
        }

        match self.config.default_policy.as_str() {
            "allow" => PolicyDecision::Allow,
            "deny" => PolicyDecision::Deny(format!("Tool {} not in allow list", tool_name)),
            _ => PolicyDecision::Deny(format!("Unknown policy: {}", self.config.default_policy)),
        }
    }
}

pub struct ToolRouter {
    registry: Arc<ToolRegistry>,
    policy: Arc<dyn PolicyEngine>,
}

impl ToolRouter {
    pub fn new(registry: Arc<ToolRegistry>, policy: Arc<dyn PolicyEngine>) -> Self {
        Self { registry, policy }
    }

    pub fn with_default_policy(registry: Arc<ToolRegistry>) -> Self {
        let policy = DefaultPolicyEngine::new(ToolConfigStub::default());
        Self {
            registry,
            policy: Arc::new(policy),
        }
    }

    pub async fn route(
        &self,
        tool_name: &str,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let decision = self.policy.decide(tool_name, context).await;

        match decision {
            PolicyDecision::Deny(reason) => {
                warn!("Tool {} denied: {}", tool_name, reason);
                Err(ToolError::PermissionDenied(reason))
            }
            PolicyDecision::NeedsApproval(reason) => {
                info!("Tool {} requires approval: {}", tool_name, reason);
                Err(ToolError::PermissionDenied(format!(
                    "Approval required: {}",
                    reason
                )))
            }
            PolicyDecision::Allow => {
                debug!("Tool {} allowed by policy", tool_name);
                self.registry.execute(tool_name, args, context).await
            }
        }
    }

    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }
}
