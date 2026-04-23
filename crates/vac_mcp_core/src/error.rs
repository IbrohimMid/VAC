use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum McpCoreError {
    #[error("config: {0}")]
    Config(String),
    #[error("invalid state transition: {from:?} → {to:?}")]
    InvalidTransition {
        from: crate::state::McpConnectionState,
        to: crate::state::McpConnectionState,
    },
    #[error("other: {0}")]
    Other(String),
}

pub type McpCoreResult<T> = std::result::Result<T, McpCoreError>;
