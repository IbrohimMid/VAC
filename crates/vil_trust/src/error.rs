use thiserror::Error;

#[derive(Error, Debug)]
pub enum TrustError {
    #[error("Permission denied: {action} requires {required}")]
    PermissionDenied { action: String, required: String },

    #[error("Trust zone violation: agent '{agent}' in zone '{zone}' cannot {action}")]
    ZoneViolation {
        agent: String,
        zone: String,
        action: String,
    },

    #[error("Policy error: {0}")]
    Policy(String),

    #[error("Invalid configuration: {0}")]
    Config(String),
}

pub type TrustResult<T> = Result<T, TrustError>;
