//! MCP Client — connects to external MCP servers via stdio or SSE transport.

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::ToolError;
use crate::registry::ToolRegistry;
use async_trait::async_trait;
use crate::registry::{ToolContext, VilTool};
use super::{JsonRpcRequest, JsonRpcResponse, McpServerConfig, McpToolDef, McpTransport};

pub struct McpClient {
    config: McpServerConfig,
    registry: Arc<ToolRegistry>,
    connection: Arc<Mutex<Option<McpConnection>>>,
    request_id: Arc<Mutex<u64>>,
}

enum McpConnection {
    Stdio {
        child: Child,
        stdin: tokio::process::ChildStdin,
        stdout: BufReader<tokio::process::ChildStdout>,
    },
    Sse {
        base_url: String,
        http: reqwest::Client,
    },
}

impl McpClient {
    pub fn new(config: McpServerConfig, registry: Arc<ToolRegistry>) -> Self {
        Self {
            config,
            registry,
            connection: Arc::new(Mutex::new(None)),
            request_id: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn connect(&self) -> Result<(), ToolError> {
        info!(name = %self.config.name, "Connecting to MCP server");

        let conn = match &self.config.transport {
            McpTransport::Stdio { command, args } => {
                let mut cmd = Command::new(command);
                cmd.args(args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null());

                for (k, v) in &self.config.env {
                    cmd.env(k, v);
                }

                let mut child = cmd.spawn()
                    .map_err(|e| ToolError::McpError(format!("Failed to spawn '{}': {}", command, e)))?;

                let stdin = child.stdin.take()
                    .ok_or_else(|| ToolError::McpError("Failed to get stdin".into()))?;
                let stdout = child.stdout.take()
                    .ok_or_else(|| ToolError::McpError("Failed to get stdout".into()))?;

                McpConnection::Stdio {
                    child,
                    stdin,
                    stdout: BufReader::new(stdout),
                }
            }
            McpTransport::Sse { url } => {
                McpConnection::Sse {
                    base_url: url.clone(),
                    http: reqwest::Client::new(),
                }
            }
        };

        *self.connection.lock().await = Some(conn);

        let _init_response = self.send_request("initialize", Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "vac",
                "version": "0.1.0"
            }
        }))).await?;

        self.send_notification("notifications/initialized", None).await?;

        info!(name = %self.config.name, "Connected to MCP server");
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), ToolError> {
        let mut conn = self.connection.lock().await;
        if let Some(McpConnection::Stdio { mut child, .. }) = conn.take() {
            let _ = child.kill().await;
        }
        info!(name = %self.config.name, "Disconnected from MCP server");
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<McpToolDef>, ToolError> {
        let response = self.send_request("tools/list", None).await?;

        let tools_value = response
            .get("tools")
            .ok_or_else(|| ToolError::McpError("No 'tools' field in response".into()))?;

        let tools: Vec<McpToolDef> = serde_json::from_value(tools_value.clone())
            .map_err(|e| ToolError::McpError(format!("Failed to parse tools: {}", e)))?;

        debug!(count = tools.len(), "Discovered MCP tools");
        Ok(tools)
    }

    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let response = self.send_request("tools/call", Some(serde_json::json!({
            "name": name,
            "arguments": args,
        }))).await?;

        Ok(response)
    }

    pub async fn is_connected(&self) -> bool {
        self.connection.lock().await.is_some()
    }

    pub async fn register_proxy_tools(&self) -> Result<usize, ToolError> {
        let tools = self.list_tools().await?;
        let count = tools.len();
        let prefix = &self.config.name;

        for tool_def in tools {
            let proxy = McpProxyTool {
                server_name: prefix.clone(),
                tool_name: tool_def.name.clone(),
                tool_description: tool_def.description.unwrap_or_default(),
                tool_schema: tool_def.input_schema.unwrap_or(serde_json::json!({"type": "object"})),
                client_connection: self.connection.clone(),
                request_id: self.request_id.clone(),
            };
            let prefixed_name = format!("{}_{}", prefix, tool_def.name);
            info!(name = %prefixed_name, "Registering MCP proxy tool");
            self.registry.register(proxy).await?;
        }

        Ok(count)
    }

    async fn next_id(&self) -> u64 {
        let mut id = self.request_id.lock().await;
        *id += 1;
        *id
    }

    async fn send_request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value, ToolError> {
        let id = self.next_id().await;
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| ToolError::McpError(format!("Serialize error: {}", e)))?;

        debug!(method = %method, id = id, "Sending MCP request");

        let mut conn = self.connection.lock().await;
        let conn = conn.as_mut()
            .ok_or_else(|| ToolError::McpError("Not connected".into()))?;

        match conn {
            McpConnection::Stdio { stdin, stdout, .. } => {
                stdin.write_all(request_json.as_bytes()).await
                    .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
                stdin.write_all(b"\n").await
                    .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
                stdin.flush().await
                    .map_err(|e| ToolError::McpError(format!("Flush error: {}", e)))?;

                let mut line = String::new();
                stdout.read_line(&mut line).await
                    .map_err(|e| ToolError::McpError(format!("Read error: {}", e)))?;

                let response: JsonRpcResponse = serde_json::from_str(line.trim())
                    .map_err(|e| ToolError::McpError(format!("Parse error: {}", e)))?;

                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!("MCP error {}: {}", err.code, err.message)));
                }

                response.result.ok_or_else(|| ToolError::McpError("No result in response".into()))
            }
            McpConnection::Sse { base_url, http } => {
                let url = base_url.clone();
                let resp = http.post(url.as_str()).json(&request).send().await
                    .map_err(|e| ToolError::McpError(format!("HTTP: {}", e)))?;
                let body = resp.text().await
                    .map_err(|e| ToolError::McpError(format!("Read: {}", e)))?;
                let response: JsonRpcResponse = serde_json::from_str(&body)
                    .map_err(|e| ToolError::McpError(format!("Parse: {}", e)))?;
                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!("{}: {}", err.code, err.message)));
                }
                response.result.ok_or_else(|| ToolError::McpError("No result".into()))
            }
        }
    }

    async fn send_notification(&self, method: &str, params: Option<serde_json::Value>) -> Result<(), ToolError> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });

        let json = serde_json::to_string(&notification)
            .map_err(|e| ToolError::McpError(format!("Serialize error: {}", e)))?;

        let mut conn = self.connection.lock().await;
        if let Some(McpConnection::Stdio { stdin, .. }) = conn.as_mut() {
            stdin.write_all(json.as_bytes()).await
                .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
            stdin.write_all(b"\n").await
                .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
            stdin.flush().await
                .map_err(|e| ToolError::McpError(format!("Flush error: {}", e)))?;
        }
        Ok(())
    }
}

struct McpProxyTool {
    server_name: String,
    tool_name: String,
    tool_description: String,
    tool_schema: serde_json::Value,
    client_connection: Arc<Mutex<Option<McpConnection>>>,
    request_id: Arc<Mutex<u64>>,
}

#[async_trait]
impl VilTool for McpProxyTool {
    fn name(&self) -> &str {
        &self.tool_name
    }
    fn description(&self) -> &str { &self.tool_description }
    fn input_schema(&self) -> serde_json::Value { self.tool_schema.clone() }
    fn trust_requirement(&self) -> &str { "trusted" }
    fn risk_level(&self) -> &str { "needs_approval" }

    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<serde_json::Value, ToolError> {
        let mut id_lock = self.request_id.lock().await;
        *id_lock += 1;
        let id = *id_lock;
        drop(id_lock);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": self.tool_name,
                "arguments": args,
            })),
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| ToolError::McpError(format!("Serialize: {}", e)))?;

        let mut conn = self.client_connection.lock().await;
        let conn = conn.as_mut()
            .ok_or_else(|| ToolError::McpError("MCP server not connected".into()))?;

        match conn {
            McpConnection::Stdio { stdin, stdout, .. } => {
                stdin.write_all(request_json.as_bytes()).await
                    .map_err(|e| ToolError::McpError(format!("Write: {}", e)))?;
                stdin.write_all(b"\n").await.map_err(|e| ToolError::McpError(format!("Write: {}", e)))?;
                stdin.flush().await.map_err(|e| ToolError::McpError(format!("Flush: {}", e)))?;

                let mut line = String::new();
                stdout.read_line(&mut line).await
                    .map_err(|e| ToolError::McpError(format!("Read: {}", e)))?;

                let response: JsonRpcResponse = serde_json::from_str(line.trim())
                    .map_err(|e| ToolError::McpError(format!("Parse: {}", e)))?;

                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!("{}: {}", err.code, err.message)));
                }
                response.result.ok_or_else(|| ToolError::McpError("No result".into()))
            }
            McpConnection::Sse { base_url, http } => {
                let url = base_url.clone();
                let resp = http.post(url.as_str()).json(&request).send().await
                    .map_err(|e| ToolError::McpError(format!("HTTP: {}", e)))?;
                let body = resp.text().await
                    .map_err(|e| ToolError::McpError(format!("Read: {}", e)))?;
                let response: JsonRpcResponse = serde_json::from_str(&body)
                    .map_err(|e| ToolError::McpError(format!("Parse: {}", e)))?;
                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!("{}: {}", err.code, err.message)));
                }
                response.result.ok_or_else(|| ToolError::McpError("No result".into()))
            }
        }
    }
}