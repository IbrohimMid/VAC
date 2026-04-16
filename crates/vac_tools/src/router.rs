use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

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
    registry: Option<Arc<ToolRegistry>>,
    config: Option<Arc<ToolConfigStub>>,
}

impl VilTrustPolicyAdapter {
    pub fn new() -> Self {
        Self {
            engine: vil_trust::PolicyEngine::default(), // Uses 3 default rules
            registry: None,
            config: None,
        }
    }

    pub fn with_registry_and_config(registry: Arc<ToolRegistry>, config: ToolConfigStub) -> Self {
        Self {
            engine: vil_trust::PolicyEngine::default(),
            registry: Some(registry),
            config: Some(Arc::new(config)),
        }
    }

    pub fn with_all(
        engine: vil_trust::PolicyEngine,
        registry: Arc<ToolRegistry>,
        config: ToolConfigStub,
    ) -> Self {
        Self {
            engine,
            registry: Some(registry),
            config: Some(Arc::new(config)),
        }
    }

    /// Parse risk_level string from VilTool trait into vil_trust::RiskLevel
    #[allow(dead_code)]
    fn parse_risk_level(level: &str) -> RiskLevel {
        match level.to_lowercase().as_str() {
            "safe" => RiskLevel::Safe,
            "medium" => RiskLevel::NeedsApproval, // Medium requires approval but not dangerous
            "needs_approval" | "needsapproval" => RiskLevel::NeedsApproval,
            "dangerous" => RiskLevel::Dangerous,
            _ => RiskLevel::NeedsApproval, // Default to cautious
        }
    }
}

impl Default for VilTrustPolicyAdapter {
    fn default() -> Self {
        Self::new()
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
        if let Some(config) = &self.config {
            if let Some(&allowed) = config.allow.get(tool_name) {
                if allowed {
                    debug!("Tool {} explicitly allowed by config", tool_name);
                    return PolicyDecision::Allow;
                }
            }
            if let Some(&denied) = config.deny.get(tool_name) {
                if denied {
                    warn!("Tool {} explicitly denied by config", tool_name);
                    return PolicyDecision::Deny(format!(
                        "Tool {} is explicitly denied",
                        tool_name
                    ));
                }
            }
        }

        let risk_level = if let Some(registry) = &self.registry {
            if let Some(tool) = registry.get(tool_name).await {
                Self::parse_risk_level(tool.risk_level())
            } else {
                classify_tool_risk(tool_name)
            }
        } else {
            classify_tool_risk(tool_name)
        };

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
        "file_read" | "glob" | "grep" | "search" | "todo_write" | "task_done" | "vil_knowledge" => {
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
        if let Some(definition) = self.registry.get_definition(tool_name).await
            && let Some(reason) = environment_mode_denial_reason(
                &context.environment_mode,
                &definition.trust_requirement,
                tool_name,
            )
        {
            warn!(
                tool = %tool_name,
                environment_mode = %context.environment_mode,
                trust_requirement = %definition.trust_requirement,
                "Tool denied by environment mode"
            );
            return Err(ToolError::PermissionDenied(reason));
        }

        let decision = self.policy.decide(tool_name, &args, context).await;

        match decision {
            PolicyDecision::Deny(reason) => {
                warn!("Tool {} denied: {}", tool_name, reason);
                Err(ToolError::PermissionDenied(reason))
            }
            PolicyDecision::NeedsApproval(reason) => {
                use crate::registry::AgentZone;
                if context.agent_zone == AgentZone::SandboxedSubagent {
                    warn!(%tool_name, "Sandboxed subagent denied needs-approval tool");
                    return Err(ToolError::PermissionDenied(format!(
                        "Tool '{}' requires approval — denied in sandboxed subagent zone",
                        tool_name
                    )));
                }
                // Return ApprovalRequired error to be captured by executor
                // This allows the executor to populate pending_approvals
                warn!(%tool_name, %reason, "Tool requires approval");
                Err(ToolError::ApprovalRequired(reason))
            }
            PolicyDecision::Allow => {
                debug!("Tool {} allowed by policy", tool_name);
                // Restore secrets before execution
                let restored_args = {
                    let privacy = context.privacy.read().await;
                    privacy.restore_value(args)
                };
                let result = self
                    .registry
                    .execute(tool_name, restored_args, context)
                    .await;
                // Substitute secrets in result
                match result {
                    Ok(v) => {
                        let mut privacy = context.privacy.write().await;
                        Ok(privacy.substitute_value(v))
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub async fn route_approved(
        &self,
        tool_name: &str,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        // Restore secrets before execution
        let restored_args = {
            let privacy = context.privacy.read().await;
            privacy.restore_value(args)
        };
        let result = self
            .registry
            .execute(tool_name, restored_args, context)
            .await;
        // Substitute secrets in result
        match result {
            Ok(v) => {
                let mut privacy = context.privacy.write().await;
                Ok(privacy.substitute_value(v))
            }
            Err(e) => Err(e),
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
                | "vil_diagnostics"
                | "vil_knowledge"
                | "vil_status"
                | "vil_lsp_query"
        )
    }
}

fn environment_mode_denial_reason(
    environment_mode: &str,
    trust_requirement: &str,
    tool_name: &str,
) -> Option<String> {
    let is_mcp_remote_verified = trust_requirement == "mcp-remote-verified";
    let is_mcp_remote_untrusted = trust_requirement == "mcp-remote-untrusted";

    match environment_mode {
        "restricted-offline" if is_mcp_remote_verified || is_mcp_remote_untrusted => Some(format!(
            "Tool '{}' denied in restricted-offline mode: remote MCP tools are disabled",
            tool_name
        )),
        "isolated" | "trusted-networked" if is_mcp_remote_untrusted => Some(format!(
            "Tool '{}' denied in {} mode: untrusted remote MCP tools are disabled",
            tool_name, environment_mode
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{AgentZone, VilTool};
    use async_trait::async_trait;
    use serde_json::json;

    struct MockNeedsApprovalPolicy;

    #[async_trait]
    impl PolicyEngine for MockNeedsApprovalPolicy {
        async fn decide(
            &self,
            _tool_name: &str,
            _args: &serde_json::Value,
            _context: &ToolContext,
        ) -> PolicyDecision {
            PolicyDecision::NeedsApproval("mock reason".to_string())
        }
    }

    struct PrivacyMockTool;

    #[async_trait]
    impl VilTool for PrivacyMockTool {
        fn name(&self) -> &str {
            "privacy_mock_tool"
        }

        fn description(&self) -> &str {
            "A mock tool to test privacy restoration and substitution"
        }

        fn input_schema(&self) -> serde_json::Value {
            json!({})
        }

        fn trust_requirement(&self) -> &str {
            "none"
        }

        fn risk_level(&self) -> &str {
            "safe"
        }

        async fn execute(
            &self,
            args: serde_json::Value,
            _context: &ToolContext,
        ) -> Result<serde_json::Value, ToolError> {
            // Verify that the args have been correctly restored from alias to the actual secret
            assert_eq!(
                args,
                json!("192.168.1.1"),
                "Args were not restored correctly"
            );

            // Return a new secret that should be substituted by the router before returning
            Ok(json!("sk-abcdefghijklmnopqrstuvwxyz1234567890"))
        }
    }

    #[tokio::test]
    async fn test_route_needs_approval_in_sandboxed_subagent_zone() {
        let registry = Arc::new(ToolRegistry::new());
        let policy = Arc::new(MockNeedsApprovalPolicy);
        let router = ToolRouter::new(registry, policy);

        let mut context = ToolContext::new("/tmp".into());
        context.agent_zone = AgentZone::SandboxedSubagent;

        let args = serde_json::json!({});
        let result = router.route("test_tool", args, &context).await;

        match result {
            Err(ToolError::PermissionDenied(reason)) => {
                assert!(reason.contains("denied in sandboxed subagent zone"));
            }
            other => panic!("Expected Err(ToolError::PermissionDenied), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_router_privacy_route_allow() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(PrivacyMockTool).await.unwrap();

        let mut config = ToolConfigStub::default();
        config.allow.insert("privacy_mock_tool".to_string(), true);

        let policy = Arc::new(DefaultPolicyEngine::new(config));
        let router = ToolRouter::new(registry, policy);

        let context = ToolContext::new(std::path::PathBuf::from("."));

        let alias = {
            let mut privacy = context.privacy.write().await;
            privacy.substitute("192.168.1.1")
        };

        // Ensure the alias is actually an alias and not the secret itself
        assert_ne!(alias, "192.168.1.1");
        assert!(alias.starts_with("SECRET_"));

        let result = router
            .route("privacy_mock_tool", json!(alias), &context)
            .await
            .unwrap();

        // Ensure the result has been substituted
        assert_ne!(result, json!("sk-abcdefghijklmnopqrstuvwxyz1234567890"));
        assert!(result.as_str().unwrap().starts_with("SECRET_API_KEY_"));
    }

    #[tokio::test]
    async fn test_router_privacy_route_approved() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(PrivacyMockTool).await.unwrap();

        // Policy engine doesn't matter for route_approved, but we need it for router initialization
        let config = ToolConfigStub::default();
        let policy = Arc::new(DefaultPolicyEngine::new(config));
        let router = ToolRouter::new(registry, policy);

        let context = ToolContext::new(std::path::PathBuf::from("."));

        let alias = {
            let mut privacy = context.privacy.write().await;
            privacy.substitute("192.168.1.1")
        };

        assert_ne!(alias, "192.168.1.1");
        assert!(alias.starts_with("SECRET_"));

        let result = router
            .route_approved("privacy_mock_tool", json!(alias), &context)
            .await
            .unwrap();

        // Ensure the result has been substituted
        assert_ne!(result, json!("sk-abcdefghijklmnopqrstuvwxyz1234567890"));
        assert!(result.as_str().unwrap().starts_with("SECRET_API_KEY_"));
    }

    #[tokio::test]
    async fn restricted_offline_denies_remote_verified_mcp_tool() {
        struct RemoteVerifiedTool;

        #[async_trait]
        impl VilTool for RemoteVerifiedTool {
            fn name(&self) -> &str {
                "remote_verified_tool"
            }

            fn description(&self) -> &str {
                "remote verified"
            }

            fn input_schema(&self) -> serde_json::Value {
                json!({})
            }

            fn trust_requirement(&self) -> &str {
                "mcp-remote-verified"
            }

            fn risk_level(&self) -> &str {
                "safe"
            }

            async fn execute(
                &self,
                _args: serde_json::Value,
                _context: &ToolContext,
            ) -> Result<serde_json::Value, ToolError> {
                Ok(json!({"ok": true}))
            }
        }

        let registry = Arc::new(ToolRegistry::new());
        registry.register(RemoteVerifiedTool).await.unwrap();

        let mut config = ToolConfigStub::default();
        config
            .allow
            .insert("remote_verified_tool".to_string(), true);
        let router = ToolRouter::new(registry, Arc::new(DefaultPolicyEngine::new(config)));

        let context = ToolContext::new(std::path::PathBuf::from("."))
            .with_environment_mode("restricted-offline");
        let result = router
            .route("remote_verified_tool", json!({}), &context)
            .await;

        match result {
            Err(ToolError::PermissionDenied(reason)) => {
                assert!(reason.contains("restricted-offline"));
            }
            other => panic!("expected restricted-offline denial, got {:?}", other),
        }
    }
}
