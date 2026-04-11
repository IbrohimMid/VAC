use thiserror::Error;

#[derive(Error, Debug)]
pub enum InferenceError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type InferenceResult<T> = Result<T, InferenceError>;
