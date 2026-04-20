//! VWFD loader errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VwfdError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("unsupported apiVersion: {0} (expected vil.vastar.io/v1)")]
    UnsupportedApiVersion(String),

    #[error("unknown execution mode: {0}")]
    UnknownExecutionMode(String),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
