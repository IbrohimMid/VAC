use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum McpCoreError {
    #[error("config: {0}")]
    Config(String),
    #[error(
        "invalid state transition from {from:?} on event {attempted:?}"
    )]
    InvalidTransition {
        /// State at the time the transition was attempted.
        from: crate::state::McpConnectionState,
        /// Event the caller tried to apply.
        attempted: crate::state::StateTransition,
    },
    #[error("other: {0}")]
    Other(String),
    /// W4.1 — elicitation handler reported a protocol violation.
    #[error("mcp protocol: {0}")]
    Protocol(String),
}

pub type McpCoreResult<T> = std::result::Result<T, McpCoreError>;
