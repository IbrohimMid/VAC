use thiserror::Error;

pub type ToolResult<T> = Result<T, ToolError>;

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

    #[error("Warden blocked: {0}")]
    WardenBlocked(String),

    #[error("Approval required: {0}")]
    ApprovalRequired(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Request error: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("Glob pattern error: {0}")]
    GlobPatternError(#[from] glob::PatternError),

    #[error("Glob error: {0}")]
    GlobError(#[from] glob::GlobError),

    #[error("Ignore error: {0}")]
    IgnoreError(#[from] ignore::Error),

    #[error("Grep regex error: {0}")]
    GrepRegexError(#[from] grep_regex::Error),
}
