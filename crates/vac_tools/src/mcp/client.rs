//! MCP Client — connects to external MCP servers via stdio or SSE transport.

use reqwest::{Certificate, Identity};
use std::fs;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::{
    JsonRpcRequest, JsonRpcResponse, McpServerConfig, McpTlsConfig, McpToolDef, McpTransport,
    McpTrustClass,
};
use crate::error::ToolError;
use crate::registry::ToolRegistry;
use crate::registry::{ToolContext, VilTool};
use async_trait::async_trait;

pub struct McpClient {
    config: McpServerConfig,
    registry: Arc<ToolRegistry>,
    connection: Arc<Mutex<Option<McpConnection>>>,
    request_id: Arc<Mutex<u64>>,
}

#[allow(clippy::large_enum_variant)]
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
        self.validate_config()?;
        let trust_class = self.config.effective_trust_class();
        info!(
            name = %self.config.name,
            trust = trust_class.as_label(),
            "Connecting to MCP server"
        );

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

                let mut child = cmd.spawn().map_err(|e| {
                    ToolError::McpError(format!("Failed to spawn '{}': {}", command, e))
                })?;

                let stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| ToolError::McpError("Failed to get stdin".into()))?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| ToolError::McpError("Failed to get stdout".into()))?;

                McpConnection::Stdio {
                    child,
                    stdin,
                    stdout: BufReader::new(stdout),
                }
            }
            McpTransport::Sse { url } => McpConnection::Sse {
                base_url: url.clone(),
                http: self.build_sse_client(url, trust_class)?,
            },
        };

        *self.connection.lock().await = Some(conn);

        let _init_response = self
            .send_request(
                "initialize",
                Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "vac",
                        "version": "0.1.0"
                    }
                })),
            )
            .await?;

        self.send_notification("notifications/initialized", None)
            .await?;

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

    pub async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        let response = self
            .send_request(
                "tools/call",
                Some(serde_json::json!({
                    "name": name,
                    "arguments": args,
                })),
            )
            .await?;

        Ok(response)
    }

    pub async fn is_connected(&self) -> bool {
        self.connection.lock().await.is_some()
    }

    pub async fn register_proxy_tools(&self) -> Result<usize, ToolError> {
        let tools = self.list_tools().await?;
        let count = tools.len();
        let prefix = &self.config.name;
        let trust_requirement = self
            .config
            .effective_trust_class()
            .proxy_trust_requirement()
            .to_string();

        for tool_def in tools {
            let prefixed_name = format!("{}_{}", prefix, tool_def.name);
            let proxy = McpProxyTool {
                prefixed_name: prefixed_name.clone(),
                tool_name: tool_def.name.clone(),
                tool_description: tool_def.description.unwrap_or_default(),
                tool_schema: tool_def
                    .input_schema
                    .unwrap_or(serde_json::json!({"type": "object"})),
                trust_requirement: trust_requirement.clone(),
                client_connection: self.connection.clone(),
                request_id: self.request_id.clone(),
            };
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

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ToolError> {
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
        let conn = conn
            .as_mut()
            .ok_or_else(|| ToolError::McpError("Not connected".into()))?;

        match conn {
            McpConnection::Stdio { stdin, stdout, .. } => {
                stdin
                    .write_all(request_json.as_bytes())
                    .await
                    .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| ToolError::McpError(format!("Flush error: {}", e)))?;

                let mut line = String::new();
                stdout
                    .read_line(&mut line)
                    .await
                    .map_err(|e| ToolError::McpError(format!("Read error: {}", e)))?;

                let response: JsonRpcResponse = serde_json::from_str(line.trim())
                    .map_err(|e| ToolError::McpError(format!("Parse error: {}", e)))?;

                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!(
                        "MCP error {}: {}",
                        err.code, err.message
                    )));
                }

                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result in response".into()))
            }
            McpConnection::Sse { base_url, http } => {
                let url = base_url.clone();
                let resp = http
                    .post(url.as_str())
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| ToolError::McpError(format!("HTTP: {}", e)))?;
                let body = resp
                    .text()
                    .await
                    .map_err(|e| ToolError::McpError(format!("Read: {}", e)))?;
                let response: JsonRpcResponse = serde_json::from_str(&body)
                    .map_err(|e| ToolError::McpError(format!("Parse: {}", e)))?;
                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!(
                        "{}: {}",
                        err.code, err.message
                    )));
                }
                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result".into()))
            }
        }
    }

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), ToolError> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });

        let json = serde_json::to_string(&notification)
            .map_err(|e| ToolError::McpError(format!("Serialize error: {}", e)))?;

        let mut conn = self.connection.lock().await;
        if let Some(McpConnection::Stdio { stdin, .. }) = conn.as_mut() {
            stdin
                .write_all(json.as_bytes())
                .await
                .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| ToolError::McpError(format!("Write error: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| ToolError::McpError(format!("Flush error: {}", e)))?;
        }
        Ok(())
    }

    fn validate_config(&self) -> Result<(), ToolError> {
        let trust_class = self.config.effective_trust_class();

        match (&self.config.transport, trust_class) {
            (McpTransport::Stdio { .. }, McpTrustClass::LocalTrusted) => Ok(()),
            (McpTransport::Stdio { .. }, other) => Err(ToolError::McpError(format!(
                "stdio MCP server '{}' cannot use trust class {:?}",
                self.config.name, other
            ))),
            (McpTransport::Sse { url }, trust_class) => {
                let parsed = reqwest::Url::parse(url).map_err(|e| {
                    ToolError::McpError(format!("Invalid MCP URL for '{}': {e}", self.config.name))
                })?;
                let host = parsed.host_str().unwrap_or_default();
                let is_localhost = matches!(host, "localhost" | "127.0.0.1" | "::1");

                if trust_class == McpTrustClass::LocalTrusted && !is_localhost {
                    return Err(ToolError::McpError(format!(
                        "SSE MCP server '{}' uses LocalTrusted but host '{host}' is not local",
                        self.config.name
                    )));
                }

                if trust_class == McpTrustClass::RemoteVerified && parsed.scheme() != "https" {
                    return Err(ToolError::McpError(format!(
                        "RemoteVerified MCP server '{}' must use https",
                        self.config.name
                    )));
                }

                if let Some(tls) = &self.config.tls {
                    self.validate_tls_config(host, tls)?;
                } else if trust_class == McpTrustClass::RemoteVerified {
                    warn!(
                        name = %self.config.name,
                        "RemoteVerified MCP server has no explicit TLS overrides; using system roots"
                    );
                }

                Ok(())
            }
        }
    }

    fn validate_tls_config(&self, host: &str, tls: &McpTlsConfig) -> Result<(), ToolError> {
        if let Some(server_name) = &tls.server_name
            && server_name != host
        {
            return Err(ToolError::McpError(format!(
                "MCP server '{}' server_name '{}' does not match URL host '{}'",
                self.config.name, server_name, host
            )));
        }

        if tls.require_mtls && (tls.client_cert_file.is_none() || tls.client_key_file.is_none()) {
            return Err(ToolError::McpError(format!(
                "MCP server '{}' requires mTLS but client cert/key is missing",
                self.config.name
            )));
        }

        Ok(())
    }

    fn build_sse_client(
        &self,
        _url: &str,
        _trust_class: McpTrustClass,
    ) -> Result<reqwest::Client, ToolError> {
        let mut builder = reqwest::Client::builder();

        if let Some(tls) = &self.config.tls {
            if let Some(ca_file) = &tls.ca_file {
                let pem = fs::read(ca_file).map_err(|e| {
                    ToolError::McpError(format!(
                        "Failed to read CA file for MCP server '{}': {e}",
                        self.config.name
                    ))
                })?;
                let cert = Certificate::from_pem(&pem).map_err(|e| {
                    ToolError::McpError(format!(
                        "Failed to parse CA file for MCP server '{}': {e}",
                        self.config.name
                    ))
                })?;
                builder = builder.add_root_certificate(cert);
            }

            if let (Some(cert_file), Some(key_file)) = (&tls.client_cert_file, &tls.client_key_file)
            {
                let mut pem = fs::read(cert_file).map_err(|e| {
                    ToolError::McpError(format!(
                        "Failed to read client cert for MCP server '{}': {e}",
                        self.config.name
                    ))
                })?;
                let key = fs::read(key_file).map_err(|e| {
                    ToolError::McpError(format!(
                        "Failed to read client key for MCP server '{}': {e}",
                        self.config.name
                    ))
                })?;
                pem.extend(key);
                let identity = Identity::from_pem(&pem).map_err(|e| {
                    ToolError::McpError(format!(
                        "Failed to parse client identity for MCP server '{}': {e}",
                        self.config.name
                    ))
                })?;
                builder = builder.identity(identity);
            }
        }

        builder.build().map_err(|e| {
            ToolError::McpError(format!(
                "Failed to build HTTP client for MCP server '{}': {e}",
                self.config.name
            ))
        })
    }
}

struct McpProxyTool {
    prefixed_name: String,
    tool_name: String,
    tool_description: String,
    tool_schema: serde_json::Value,
    trust_requirement: String,
    client_connection: Arc<Mutex<Option<McpConnection>>>,
    request_id: Arc<Mutex<u64>>,
}

#[async_trait]
impl VilTool for McpProxyTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        &self.prefixed_name
    }
    fn description(&self) -> &str {
        &self.tool_description
    }
    fn input_schema(&self) -> serde_json::Value {
        self.tool_schema.clone()
    }
    fn trust_requirement(&self) -> &str {
        &self.trust_requirement
    }
    fn risk_level(&self) -> &str {
        "medium"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
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
        let conn = conn
            .as_mut()
            .ok_or_else(|| ToolError::McpError("MCP server not connected".into()))?;

        match conn {
            McpConnection::Stdio { stdin, stdout, .. } => {
                stdin
                    .write_all(request_json.as_bytes())
                    .await
                    .map_err(|e| ToolError::McpError(format!("Write: {}", e)))?;
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(|e| ToolError::McpError(format!("Write: {}", e)))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| ToolError::McpError(format!("Flush: {}", e)))?;

                let mut line = String::new();
                stdout
                    .read_line(&mut line)
                    .await
                    .map_err(|e| ToolError::McpError(format!("Read: {}", e)))?;

                let response: JsonRpcResponse = serde_json::from_str(line.trim())
                    .map_err(|e| ToolError::McpError(format!("Parse: {}", e)))?;

                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!(
                        "{}: {}",
                        err.code, err.message
                    )));
                }
                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result".into()))
            }
            McpConnection::Sse { base_url, http } => {
                let url = base_url.clone();
                let resp = http
                    .post(url.as_str())
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| ToolError::McpError(format!("HTTP: {}", e)))?;
                let body = resp
                    .text()
                    .await
                    .map_err(|e| ToolError::McpError(format!("Read: {}", e)))?;
                let response: JsonRpcResponse = serde_json::from_str(&body)
                    .map_err(|e| ToolError::McpError(format!("Parse: {}", e)))?;
                if let Some(err) = response.error {
                    return Err(ToolError::McpError(format!(
                        "{}: {}",
                        err.code, err.message
                    )));
                }
                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result".into()))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn registry() -> Arc<ToolRegistry> {
        Arc::new(ToolRegistry::new())
    }

    #[test]
    fn remote_verified_requires_https() {
        let client = McpClient::new(
            McpServerConfig {
                name: "remote".to_string(),
                transport: McpTransport::Sse {
                    url: "http://example.com/mcp".to_string(),
                },
                env: HashMap::new(),
                trust_class: Some(McpTrustClass::RemoteVerified),
                tls: None,
                approval_policy: None,
                allowed_in_modes: vec![],
            },
            registry(),
        );
        assert!(client.validate_config().is_err());
    }

    #[test]
    fn local_trusted_remote_host_is_rejected() {
        let client = McpClient::new(
            McpServerConfig {
                name: "remote".to_string(),
                transport: McpTransport::Sse {
                    url: "https://example.com/mcp".to_string(),
                },
                env: HashMap::new(),
                trust_class: Some(McpTrustClass::LocalTrusted),
                tls: None,
                approval_policy: None,
                allowed_in_modes: vec![],
            },
            registry(),
        );
        assert!(client.validate_config().is_err());
    }
}
