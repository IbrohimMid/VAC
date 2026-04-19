//! Error types returned by the trust / policy subsystem.

use thiserror::Error;

/// Errors raised by the trust subsystem.
#[derive(Error, Debug)]
pub enum TrustError {
    /// The requested action is not permitted — either denied outright or
    /// pending human approval.
    #[error("Permission denied: {action} requires {required}")]
    PermissionDenied {
        /// The action (typically a tool name) that was blocked.
        action: String,
        /// Human-readable description of what would unblock the action.
        required: String,
    },

    /// The action requires explicit human approval before proceeding.
    #[error("action '{action}' requires approval: {reason}")]
    RequiresApproval {
        /// The action (typically a tool name) that needs approval.
        action: String,
        /// Human-readable reason why approval is needed.
        reason: String,
    },

    /// The agent's trust zone does not permit the requested action.
    #[error("Trust zone violation: agent '{agent}' in zone '{zone}' cannot {action}")]
    ZoneViolation {
        /// The offending agent identifier.
        agent: String,
        /// The zone the agent is currently in.
        zone: String,
        /// The action that would have violated the zone.
        action: String,
    },

    /// An error originating from the policy engine itself.
    #[error("Policy error: {0}")]
    Policy(String),

    /// The trust configuration is malformed.
    #[error("Invalid configuration: {0}")]
    Config(String),
}

/// Convenience alias for results returned by the trust subsystem.
pub type TrustResult<T> = Result<T, TrustError>;
