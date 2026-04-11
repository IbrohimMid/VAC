use thiserror::Error;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Tool permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid tool arguments: {0}")]
    InvalidArguments(String),

    #[error("Tool registry error: {0}")]
    RegistryError(String),

    #[error("MCP error: {0}")]
    McpError(String),

    #[error("Sandbox error: {0}")]
    SandboxError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Request error: {0}")]
    RequestError(#[from] reqwest::Error),
}
