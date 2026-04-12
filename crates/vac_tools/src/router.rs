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
    async fn decide(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        context: &ToolContext,
    ) -> PolicyDecision;
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
    async fn decide(
        &self,
        tool_name: &str,
        _args: &serde_json::Value,
        _context: &ToolContext,
    ) -> PolicyDecision {
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

use vil_trust::zones::RiskLevel;

/// Adapter that bridges vac_tools::PolicyEngine trait to vil_trust::PolicyEngine
pub struct VilTrustPolicyAdapter {
    engine: vil_trust::PolicyEngine,
}

impl VilTrustPolicyAdapter {
    pub fn new() -> Self {
        Self {
            engine: vil_trust::PolicyEngine::default(), // Uses 3 default rules
        }
    }

    pub fn with_engine(engine: vil_trust::PolicyEngine) -> Self {
        Self { engine }
    }

    /// Parse risk_level string from VilTool trait into vil_trust::RiskLevel
    #[allow(dead_code)]
    fn parse_risk_level(level: &str) -> RiskLevel {
        match level.to_lowercase().as_str() {
            "safe" => RiskLevel::Safe,
            "needs_approval" | "needsapproval" => RiskLevel::NeedsApproval,
            "dangerous" => RiskLevel::Dangerous,
            _ => RiskLevel::NeedsApproval, // Default to cautious
        }
    }
}

#[async_trait]
impl PolicyEngine for VilTrustPolicyAdapter {
    async fn decide(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        context: &ToolContext,
    ) -> PolicyDecision {
        // Get tool definition to find risk_level
        // We need to look up the tool's risk level — use the tool_name to infer
        // Since we don't have access to the registry here, we use a classification function
        let risk_level = classify_tool_risk(tool_name);

        let request = vil_trust::PolicyRequest {
            tool_name: tool_name.to_string(),
            agent_id: context.session_id.to_string(),
            agent_role: "coder".to_string(),           // Default role
            agent_zone: vil_trust::TrustZone::Trusted, // Default zone for CLI
            risk_level,
            arguments_summary: args.to_string().chars().take(200).collect(),
        };

        match self.engine.evaluate(&request) {
            Ok(vil_trust::PolicyDecision::Allow) => PolicyDecision::Allow,
            Ok(vil_trust::PolicyDecision::Deny) => {
                PolicyDecision::Deny(format!("Tool '{}' denied by policy", tool_name))
            }
            Ok(vil_trust::PolicyDecision::RequireApproval) => {
                PolicyDecision::NeedsApproval(format!(
                    "Tool '{}' requires approval (risk: {:?})",
                    tool_name, request.risk_level
                ))
            }
            Err(e) => PolicyDecision::Deny(format!("Policy evaluation error: {}", e)),
        }
    }
}

/// Classify tool risk level based on tool name
/// This is the source of truth for risk classification
fn classify_tool_risk(tool_name: &str) -> RiskLevel {
    match tool_name {
        // Safe: read-only operations
        "file_read" | "glob" | "grep" | "search" | "todo" | "task_done" | "vil_knowledge" => {
            RiskLevel::Safe
        }
        // NeedsApproval: write operations
        "file_write" | "file_edit" | "git" | "cargo" => RiskLevel::NeedsApproval,
        // Dangerous: shell execution
        "bash" => RiskLevel::Dangerous,
        // Default: cautious
        _ => RiskLevel::NeedsApproval,
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
        let decision = self.policy.decide(tool_name, &args, context).await;

        match decision {
            PolicyDecision::Deny(reason) => {
                warn!("Tool {} denied: {}", tool_name, reason);
                Err(ToolError::PermissionDenied(reason))
            }
            PolicyDecision::NeedsApproval(reason) => {
                // TODO(phase4): Add interactive user approval prompt here
                info!(%tool_name, %reason, "Tool requires approval — auto-allowing in current phase");
                // For now, auto-allow NeedsApproval tools (interactive approval comes in Phase 4)
                self.registry.execute(tool_name, args, context).await
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

    /// Execute multiple tools in parallel (Data Lane) or serial (Control Lane)
    pub async fn route_batch(
        &self,
        tool_calls: Vec<(&str, serde_json::Value)>,
        context: &ToolContext,
    ) -> Vec<Result<serde_json::Value, ToolError>> {
        use futures::future::join_all;

        // Classify tools: Read tools can run parallel, Write tools serial
        let read_tools: Vec<(&str, serde_json::Value)> = tool_calls
            .iter()
            .filter(|(name, _)| self.is_read_tool(name))
            .cloned()
            .collect();

        let write_tools: Vec<(&str, serde_json::Value)> = tool_calls
            .iter()
            .filter(|(name, _)| !self.is_read_tool(name))
            .cloned()
            .collect();

        let mut results = Vec::new();

        // Execute read tools in parallel
        if !read_tools.is_empty() {
            let read_futures: Vec<_> = read_tools
                .iter()
                .map(|(name, input)| self.route(name, input.clone(), context))
                .collect();
            let read_results = join_all(read_futures).await;
            results.extend(read_results);
        }

        // Execute write tools serially
        for (name, input) in write_tools {
            let result = self.route(name, input, context).await;
            results.push(result);
        }

        results
    }

    /// Check if a tool is read-only (safe for parallel execution)
    fn is_read_tool(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "file_read"
                | "glob"
                | "grep"
                | "web_fetch"
                | "web_search"
                | "list_mcp_resources"
                | "read_mcp_resource"
                | "task_output"
                | "config"
        )
    }
}
