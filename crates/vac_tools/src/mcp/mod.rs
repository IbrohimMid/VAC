pub mod client;
pub mod server;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Sse { url } => Some(url.as_str()),
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
pub use server::McpServer;

#[cfg(test)]
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
}
