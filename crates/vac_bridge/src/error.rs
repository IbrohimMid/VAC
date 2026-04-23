use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BridgeError {
    #[error("handshake: {0}")]
    Handshake(String),
    #[error("handshake timed out after {0}ms")]
    HandshakeTimeout(u64),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("permission timed out after {0}ms")]
    PermissionTimeout(u64),
    #[error("session not attached")]
    NotAttached,
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("other: {0}")]
    Other(String),
}

pub type BridgeResult<T> = std::result::Result<T, BridgeError>;
