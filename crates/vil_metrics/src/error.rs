//! Error types for VIL Metrics.

use thiserror::Error;

/// Errors that can occur in the metrics subsystem.
#[derive(Debug, Error)]
pub enum MetricsError {
    /// Failed to collect system metrics.
    #[error("collection failed: {0}")]
    CollectionFailed(String),

    /// Invalid metric configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Storage error (e.g., retention policy).
    #[error("storage error: {0}")]
    StorageError(String),
}