//! Minimal LSP JSON-RPC protocol structs for vil-lsp communication.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

pub fn initialize_request(id: u64, root_uri: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "initialize",
        params: Some(serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "relatedInformation": false }
                }
            },
            "initializationOptions": {}
        })),
    }
}

pub fn initialized_notification() -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0",
        method: "initialized",
        params: Some(serde_json::json!({})),
    }
}

pub fn did_open_notification(uri: &str, text: &str) -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0",
        method: "textDocument/didOpen",
        params: Some(serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": text
            }
        })),
    }
}

pub fn did_change_notification(uri: &str, text: &str, version: i32) -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0",
        method: "textDocument/didChange",
        params: Some(serde_json::json!({
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [{ "text": text }]
        })),
    }
}

pub fn shutdown_request(id: u64) -> JsonRpcRequest {
    JsonRpcRequest { jsonrpc: "2.0", id, method: "shutdown", params: None }
}

pub fn exit_notification() -> JsonRpcNotification {
    JsonRpcNotification { jsonrpc: "2.0", method: "exit", params: None }
}

/// Parse a `textDocument/publishDiagnostics` params value into (uri, diagnostics).
pub fn parse_publish_diagnostics(params: &Value) -> Option<(String, Vec<Value>)> {
    let uri = params.get("uri")?.as_str()?.to_string();
    let diags = params.get("diagnostics")?.as_array()?.clone();
    Some((uri, diags))
}

/// Convert file path to LSP URI.
pub fn path_to_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

/// Convert LSP URI to file path.
pub fn uri_to_path(uri: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(uri.trim_start_matches("file://"))
}

// ── Navigation requests ───────────────────────────────────────────────────────

pub fn definition_request(id: u64, uri: &str, line: u32, character: u32) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "textDocument/definition",
        params: Some(serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        })),
    }
}

pub fn references_request(id: u64, uri: &str, line: u32, character: u32) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "textDocument/references",
        params: Some(serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": true }
        })),
    }
}

pub fn hover_request(id: u64, uri: &str, line: u32, character: u32) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "textDocument/hover",
        params: Some(serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        })),
    }
}

pub fn document_symbols_request(id: u64, uri: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "textDocument/documentSymbol",
        params: Some(serde_json::json!({ "textDocument": { "uri": uri } })),
    }
}
