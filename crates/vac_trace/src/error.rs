use thiserror::Error;

#[derive(Error, Debug)]
pub enum TraceError {
    #[error("Recording error: {0}")]
    Recording(String),

    #[error("Signing error: {0}")]
    Signing(String),

    #[error("Redaction error: {0}")]
    Redaction(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type TraceResult<T> = Result<T, TraceError>;
