use thiserror::Error;

#[derive(Error, Debug)]
pub enum RagError {
    #[error("Indexing error: {0}")]
    Indexing(String),
    #[error("Embedding error: {0}")]
    EmbeddingError(String),
    #[error("Search error: {0}")]
    Search(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type RagResult<T> = Result<T, RagError>;
