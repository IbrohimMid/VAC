use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::ToolError;

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

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError>;
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub working_dir: std::path::PathBuf,
    pub env_vars: HashMap<String, String>,
    pub session_id: uuid::Uuid,
}

impl ToolContext {
    pub fn new(working_dir: std::path::PathBuf) -> Self {
        Self {
            working_dir,
            env_vars: std::env::vars().collect(),
            session_id: uuid::Uuid::new_v4(),
        }
    }

    pub fn with_session_id(mut self, session_id: uuid::Uuid) -> Self {
        self.session_id = session_id;
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
