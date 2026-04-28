pub mod client;
pub mod presets;
pub mod server;

/// Re-export of the canonical transport/state types from
/// `vac_mcp_core`. New consumers (vac_bridge, future vac_cli/vac_tui
/// refactors) should bind these types rather than the legacy
/// `McpConnectionState` in this module, which only captures a flat
/// connected/unreachable state and is kept for existing callers.
pub mod core {
    pub use vac_mcp_core::{
        McpConfigScope, McpConnection, McpConnectionState as CoreConnectionState, McpCoreError,
        McpCoreResult, McpTransportKind, StateTransition,
    };
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpConnectionStatus {
    Connected,
    Unreachable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectionState {
    pub status: McpConnectionStatus,
    pub trust_class: Option<McpTrustClass>,
    pub allowed_in_modes: Vec<String>,
}

impl McpConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(self.status, McpConnectionStatus::Connected)
    }

    /// R3 (blueprint) — produce the `vac_mcp_core` 5-state machine
    /// record for this legacy state. Lets new consumers bind the
    /// canonical state (Disabled/Pending/Connected/Failed/NeedsAuth)
    /// without waiting for the full migration. Reason strings carry
    /// the operator-facing detail from `McpConnectionStatus::Unreachable`.
    pub fn to_core_connection(
        &self,
        server_name: impl Into<String>,
    ) -> vac_mcp_core::McpConnection {
        use vac_mcp_core::{McpConnection, McpConnectionState as CoreState};
        let (state, reason) = match &self.status {
            McpConnectionStatus::Connected => (CoreState::Connected, String::new()),
            McpConnectionStatus::Unreachable(reason) => (CoreState::Failed, reason.clone()),
        };
        McpConnection {
            server_name: server_name.into(),
            state,
            reason,
            entered_at: chrono::Utc::now(),
        }
    }
}

pub async fn probe_mcp_server(config: &McpServerConfig) -> McpConnectionState {
    let status = match &config.transport {
        McpTransport::Stdio { command, .. } => {
            // Check if command exists in PATH or is an absolute path
            let exists = if std::path::Path::new(command).is_absolute() {
                std::path::Path::new(command).exists()
            } else if let Some(paths) = std::env::var_os("PATH") {
                std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
            } else {
                false
            };

            if exists {
                McpConnectionStatus::Connected
            } else {
                McpConnectionStatus::Unreachable(format!("Command '{}' not found in PATH", command))
            }
        }
        McpTransport::Sse { url } => {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default();

            match client.head(url).send().await {
                Ok(resp)
                    if resp.status().is_success()
                        || resp.status().is_redirection()
                        || resp.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED =>
                {
                    // Some SSE endpoints might not support HEAD and return 405 Method Not Allowed,
                    // but reaching the endpoint means it's connected.
                    McpConnectionStatus::Connected
                }
                Ok(resp) => {
                    McpConnectionStatus::Unreachable(format!("HTTP error: {}", resp.status()))
                }
                Err(e) => McpConnectionStatus::Unreachable(format!("Connection failed: {}", e)),
            }
        }
        McpTransport::WebSocket { url } => {
            let parsed = reqwest::Url::parse(url).unwrap();
            let host = parsed.host_str().unwrap_or_default();
            if matches!(host, "localhost" | "127.0.0.1" | "::1") {
                McpConnectionStatus::Connected
            } else {
                McpConnectionStatus::Unreachable(
                    "Remote WebSocket currently mocked as unreachable".into(),
                )
            }
        }
    };

    McpConnectionState {
        status,
        trust_class: config.trust_class,
        allowed_in_modes: config.allowed_in_modes.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub trust_class: Option<McpTrustClass>,
    #[serde(default)]
    pub tls: Option<McpTlsConfig>,
    #[serde(default)]
    pub approval_policy: Option<String>,
    #[serde(default)]
    pub allowed_in_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpTransport {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "sse")]
    Sse { url: String },
    #[serde(rename = "websocket")]
    WebSocket { url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTrustClass {
    LocalTrusted,
    RemoteVerified,
    RemoteUntrusted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpTlsConfig {
    #[serde(default)]
    pub ca_file: Option<String>,
    #[serde(default)]
    pub client_cert_file: Option<String>,
    #[serde(default)]
    pub client_key_file: Option<String>,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub require_mtls: bool,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<serde_json::Value>,
}

impl McpTransport {
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Sse { .. })
    }

    pub fn as_url(&self) -> Option<&str> {
        match self {
            Self::Sse { url } => Some(url),
            Self::WebSocket { url } => Some(url),
            Self::Stdio { .. } => None,
        }
    }
}

impl McpTrustClass {
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::LocalTrusted => "trusted",
            Self::RemoteVerified => "verified",
            Self::RemoteUntrusted => "untrusted",
        }
    }

    pub fn proxy_trust_requirement(&self) -> &'static str {
        match self {
            Self::LocalTrusted => "mcp-local-trusted",
            Self::RemoteVerified => "mcp-remote-verified",
            Self::RemoteUntrusted => "mcp-remote-untrusted",
        }
    }
}

impl McpServerConfig {
    pub fn effective_trust_class(&self) -> McpTrustClass {
        if let Some(trust_class) = self.trust_class {
            trust_class
        } else if self.transport.is_remote() {
            McpTrustClass::RemoteUntrusted
        } else {
            McpTrustClass::LocalTrusted
        }
    }

    pub fn is_allowed_in_mode(&self, mode: &str) -> bool {
        self.allowed_in_modes.is_empty()
            || self.allowed_in_modes.iter().any(|allowed| allowed == mode)
    }
}

pub use client::McpClient;
pub use presets::{McpPresetInstanceConfig, McpServerPreset, resolve_mcp_presets};
pub use server::McpServer;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stdio_defaults_to_local_trusted() {
        let config = McpServerConfig {
            name: "local".to_string(),
            transport: McpTransport::Stdio {
                command: "echo".to_string(),
                args: vec![],
            },
            env: HashMap::new(),
            trust_class: None,
            tls: None,
            approval_policy: None,
            allowed_in_modes: vec![],
        };
        assert_eq!(config.effective_trust_class(), McpTrustClass::LocalTrusted);
    }

    #[test]
    fn remote_defaults_to_untrusted() {
        let config = McpServerConfig {
            name: "remote".to_string(),
            transport: McpTransport::Sse {
                url: "https://example.com/mcp".to_string(),
            },
            env: HashMap::new(),
            trust_class: None,
            tls: None,
            approval_policy: None,
            allowed_in_modes: vec![],
        };
        assert_eq!(
            config.effective_trust_class(),
            McpTrustClass::RemoteUntrusted
        );
    }

    #[test]
    fn to_core_connection_maps_connected() {
        let s = McpConnectionState {
            status: McpConnectionStatus::Connected,
            trust_class: None,
            allowed_in_modes: vec![],
        };
        let c = s.to_core_connection("srv");
        assert_eq!(c.state, vac_mcp_core::McpConnectionState::Connected);
        assert_eq!(c.server_name, "srv");
        assert!(c.reason.is_empty());
    }

    #[test]
    fn to_core_connection_maps_unreachable_to_failed() {
        let s = McpConnectionState {
            status: McpConnectionStatus::Unreachable("tcp reset".into()),
            trust_class: None,
            allowed_in_modes: vec![],
        };
        let c = s.to_core_connection("srv");
        assert_eq!(c.state, vac_mcp_core::McpConnectionState::Failed);
        assert_eq!(c.reason, "tcp reset");
    }
}
