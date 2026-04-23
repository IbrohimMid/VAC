use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    #[error("transcript i/o: {0}")]
    Transcript(String),
    #[error("slash command '{0}' not registered")]
    UnknownSlash(String),
    #[error("slash handler '{0}' failed: {1}")]
    SlashHandler(String, String),
    #[error("compact boundary rejected input: {0}")]
    CompactRejected(String),
    #[error("budget exceeded: used {tokens_used}, budget {budget}")]
    BudgetExceeded { tokens_used: u64, budget: u64 },
    #[error("submit cancelled")]
    Cancelled,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("other: {0}")]
    Other(String),
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;
