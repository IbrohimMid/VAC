use std::sync::Arc;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::ToolRegistry;

pub struct McpClient {
    server_url: String,
    #[allow(dead_code)]
    registry: Arc<ToolRegistry>,
}

impl McpClient {
    pub fn new(server_url: String, registry: Arc<ToolRegistry>) -> Self {
        Self {
            server_url,
            registry,
        }
    }

    pub async fn connect(&self) -> Result<(), ToolError> {
        info!("Connecting to MCP server at {}", self.server_url);
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), ToolError> {
        info!("Disconnecting from MCP server");
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<serde_json::Value>, ToolError> {
        debug!("Listing tools from MCP server");
        Ok(vec![])
    }

    pub async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        debug!("Calling MCP tool: {}", name);
        Err(ToolError::McpError("Not connected".to_string()))
    }

    pub fn is_connected(&self) -> bool {
        false
    }
}
