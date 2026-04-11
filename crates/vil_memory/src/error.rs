use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Retrieval error: {0}")]
    Retrieval(String),
    #[error("Consolidation error: {0}")]
    Consolidation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type MemoryResult<T> = Result<T, MemoryError>;
