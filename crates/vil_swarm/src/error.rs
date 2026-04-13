use thiserror::Error;

#[derive(Error, Debug)]
pub enum SwarmError {
    #[error("Agent '{0}' not found")]
    AgentNotFound(String),

    #[error("Agent '{agent}' failed: {message}")]
    AgentFailed { agent: String, message: String },

    #[error("Lane communication error: {0}")]
    LaneError(String),

    #[error("Orchestration error: {0}")]
    Orchestration(String),

    #[error("Agent loop cancelled")]
    Cancelled,

    #[error("Checkpoint error: {0}")]
    Checkpoint(String),

    #[error("Timeout: agent '{0}' exceeded deadline")]
    Timeout(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type SwarmResult<T> = Result<T, SwarmError>;
