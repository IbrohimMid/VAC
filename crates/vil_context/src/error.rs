use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContextError {
    #[error("SHM allocation failed: {0}")]
    ShmAllocation(String),
    #[error("SHM arena is full")]
    ShmFull,
    #[error("SHM access out of bounds: {0}")]
    OutOfBounds(String),
    #[error("Indexing error: {0}")]
    Indexing(String),
    #[error("Chunking error: {0}")]
    Chunking(String),
    #[error("Context retrieval error: {0}")]
    Retrieval(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type ContextResult<T> = Result<T, ContextError>;
