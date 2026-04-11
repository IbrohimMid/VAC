use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::ToolError;
use crate::registry::ToolRegistry;

pub struct McpServer {
    registry: Arc<ToolRegistry>,
    running: Arc<RwLock<bool>>,
}

impl McpServer {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self, port: u16) -> Result<(), ToolError> {
        info!("Starting MCP server on port {}", port);
        *self.running.write().await = true;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), ToolError> {
        info!("Stopping MCP server");
        *self.running.write().await = false;
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn handle_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        match method {
            "tools/list" => {
                let tools = self.registry.list().await;
                Ok(serde_json::json!({ "tools": tools }))
            }
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| ToolError::McpError("Missing tool name".to_string()))?;

                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                self.registry
                    .execute(
                        name,
                        args,
                        &crate::registry::ToolContext::new(std::path::PathBuf::from(".")),
                    )
                    .await
            }
            _ => Err(ToolError::McpError(format!("Unknown method: {}", method))),
        }
    }
}
