use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::ToolError;
use vil_context::shm::ShmArena;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub trust_requirement: String,
    pub risk_level: String,
    pub category: Option<String>,
}

#[async_trait]
pub trait VilTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn trust_requirement(&self) -> &str;
    fn risk_level(&self) -> &str;

    /// F1.1 — Return the formal `vac_tool_core::ToolSpec` for this
    /// tool. Default impl synthesizes a spec from the legacy
    /// `name/description/input_schema/trust_requirement` fields so
    /// every existing tool automatically gets a spec without a
    /// breaking change. Tools that need fine-grained capability or
    /// render hints override this.
    fn spec(&self) -> vac_tool_core::ToolSpec {
        let permission = match self.trust_requirement() {
            "safe" => vac_tool_core::ToolPermissionClass::Safe,
            "ask_once" => vac_tool_core::ToolPermissionClass::AskOnce,
            "privileged" => vac_tool_core::ToolPermissionClass::Privileged,
            _ => vac_tool_core::ToolPermissionClass::AskEveryCall,
        };
        let capability = match self.risk_level() {
            "safe" => vac_tool_core::ToolCapability::default(),
            "destructive" => vac_tool_core::ToolCapability::destructive(),
            _ => vac_tool_core::ToolCapability::mutating(),
        };
        vac_tool_core::ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.input_schema(),
            capability,
            permission,
            render: vac_tool_core::ToolRenderHints::default(),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentZone {
    #[default]
    ParentAgent,
    SandboxedSubagent,
}

#[derive(Clone)]
pub struct ToolContext {
    pub working_dir: std::path::PathBuf,
    pub env_vars: HashMap<String, String>,
    pub session_id: uuid::Uuid,
    pub shm: Option<Arc<ShmArena>>,
    pub agent_zone: AgentZone,
    pub environment_mode: String,
    pub privacy: Arc<RwLock<crate::PrivacyVault>>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("working_dir", &self.working_dir)
            .field("session_id", &self.session_id)
            .field("agent_zone", &self.agent_zone)
            .field("environment_mode", &self.environment_mode)
            .finish_non_exhaustive()
    }
}

impl ToolContext {
    pub fn new(working_dir: std::path::PathBuf) -> Self {
        Self {
            working_dir,
            env_vars: std::env::vars().collect(),
            session_id: uuid::Uuid::new_v4(),
            shm: None,
            agent_zone: AgentZone::ParentAgent,
            environment_mode: std::env::var("VAC_ENVIRONMENT_MODE")
                .unwrap_or_else(|_| "host".to_string()),
            privacy: Arc::new(RwLock::new(crate::PrivacyVault::new())),
        }
    }

    pub fn with_session_id(mut self, session_id: uuid::Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_shm(mut self, shm: Arc<ShmArena>) -> Self {
        self.shm = Some(shm);
        self
    }

    pub fn with_zone(mut self, zone: AgentZone) -> Self {
        self.agent_zone = zone;
        self
    }

    pub fn with_environment_mode(mut self, mode: impl Into<String>) -> Self {
        self.environment_mode = mode.into();
        self
    }
}

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn VilTool>>>,
    definitions: RwLock<HashMap<String, ToolDefinition>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            definitions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, tool: impl VilTool + 'static) -> Result<(), ToolError> {
        let name = tool.name().to_string();
        let definition = ToolDefinition {
            name: name.clone(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
            trust_requirement: tool.trust_requirement().to_string(),
            risk_level: tool.risk_level().to_string(),
            category: None,
        };

        debug!("Registering tool: {}", name);

        self.tools
            .write()
            .await
            .insert(name.clone(), Arc::new(tool));
        self.definitions.write().await.insert(name, definition);

        info!("Tool registered successfully");
        Ok(())
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn VilTool>> {
        self.tools.read().await.get(name).cloned()
    }

    pub async fn get_definition(&self, name: &str) -> Option<ToolDefinition> {
        self.definitions.read().await.get(name).cloned()
    }

    pub async fn list(&self) -> Vec<ToolDefinition> {
        self.definitions.read().await.values().cloned().collect()
    }

    /// F1.1 — Return every registered tool's formal `ToolSpec`.
    /// Source of truth for `ToolSearchTool` + downstream consumers
    /// (session engine, bridge) that need the full contract, not just
    /// the legacy `ToolDefinition` shape.
    pub async fn list_specs(&self) -> Vec<vac_tool_core::ToolSpec> {
        self.tools
            .read()
            .await
            .values()
            .map(|t| t.spec())
            .collect()
    }

    pub async fn list_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        self.definitions
            .read()
            .await
            .values()
            .filter(|d| d.category.as_deref() == Some(category))
            .cloned()
            .collect()
    }

    pub async fn unregister(&self, name: &str) -> Result<(), ToolError> {
        if self.tools.write().await.remove(name).is_none() {
            return Err(ToolError::NotFound(name.to_string()));
        }
        self.definitions.write().await.remove(name);
        Ok(())
    }

    pub async fn execute(
        &self,
        name: &str,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let tool = self
            .get(name)
            .await
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

        debug!("Executing tool: {}", name);
        tool.execute(args, context).await
    }
}
