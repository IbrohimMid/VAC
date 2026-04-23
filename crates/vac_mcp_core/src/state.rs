//! Connection state machine.
//!
//! Every MCP server connection is in exactly one of five states. The
//! transition table below encodes which moves are legal; callers run
//! [`McpConnection::transition`] to get validation for free.
//!
//! ```text
//!   Disabled ──enable──▶ Pending ──connect_ok──▶ Connected
//!      ▲                    │                        │
//!      │                    ├─connect_fail──▶ Failed ┘
//!      │                    └─auth_needed──▶ NeedsAuth
//!      │                                       │
//!      └──────────disable (from any)───────────┘
//! ```
//!
//! Failed / NeedsAuth / Disabled can all retry to Pending on operator
//! action. Connected can drop back to Failed or Pending on a reconnect.

use serde::{Deserialize, Serialize};

use crate::error::{McpCoreError, McpCoreResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpConnectionState {
    /// Operator has turned the server off — no I/O will happen.
    Disabled,
    /// We've dispatched a connect and are waiting on the transport.
    Pending,
    /// Handshake completed; tools are available.
    Connected,
    /// Connect returned a transport/protocol error.
    Failed,
    /// Server asked for auth; operator must supply credentials.
    NeedsAuth,
}

impl McpConnectionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Pending => "pending",
            Self::Connected => "connected",
            Self::Failed => "failed",
            Self::NeedsAuth => "needs_auth",
        }
    }

    /// Can the operator issue a retry from this state?
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Failed | Self::NeedsAuth | Self::Disabled)
    }
}

/// The event that drives a transition. Kept small on purpose —
/// consumers translate transport events into these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StateTransition {
    /// Operator flipped the server on (Disabled → Pending) or asked
    /// for a reconnect (Failed/NeedsAuth → Pending).
    Enable,
    /// Operator disabled the server (any → Disabled).
    Disable,
    /// Transport connect + handshake succeeded (Pending → Connected).
    ConnectOk,
    /// Transport or protocol error (Pending → Failed).
    ConnectFail,
    /// Server returned 401/needs-auth during handshake
    /// (Pending → NeedsAuth).
    AuthNeeded,
    /// Live connection dropped (Connected → Failed).
    Disconnect,
}

/// Full connection record: state + last-known labels + last event ts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpConnection {
    pub server_name: String,
    pub state: McpConnectionState,
    /// Short operator-facing reason attached to the last transition
    /// (error message, auth prompt target, etc.). Empty on the
    /// happy path.
    #[serde(default)]
    pub reason: String,
    /// When the current state was entered.
    pub entered_at: chrono::DateTime<chrono::Utc>,
}

impl McpConnection {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            state: McpConnectionState::Disabled,
            reason: String::new(),
            entered_at: chrono::Utc::now(),
        }
    }

    /// Attempt `transition`. Returns Err on illegal moves with the
    /// before/after states baked into the error for diagnostics.
    pub fn transition(
        &mut self,
        event: StateTransition,
        reason: impl Into<String>,
    ) -> McpCoreResult<()> {
        let next = match (self.state, event) {
            (McpConnectionState::Disabled, StateTransition::Enable) => {
                McpConnectionState::Pending
            }
            (_, StateTransition::Disable) => McpConnectionState::Disabled,
            (McpConnectionState::Pending, StateTransition::ConnectOk) => {
                McpConnectionState::Connected
            }
            (McpConnectionState::Pending, StateTransition::ConnectFail) => {
                McpConnectionState::Failed
            }
            (McpConnectionState::Pending, StateTransition::AuthNeeded) => {
                McpConnectionState::NeedsAuth
            }
            (McpConnectionState::Failed, StateTransition::Enable)
            | (McpConnectionState::NeedsAuth, StateTransition::Enable) => {
                McpConnectionState::Pending
            }
            // Operator supplied credentials and the transport reported
            // success without needing a fresh connect — NeedsAuth can
            // skip straight to Connected.
            (McpConnectionState::NeedsAuth, StateTransition::ConnectOk) => {
                McpConnectionState::Connected
            }
            // Live reconnect from Connected back into Pending, e.g.
            // after a soft restart request.
            (McpConnectionState::Connected, StateTransition::Enable) => {
                McpConnectionState::Pending
            }
            (McpConnectionState::Connected, StateTransition::Disconnect) => {
                McpConnectionState::Failed
            }
            (from, event) => {
                return Err(McpCoreError::InvalidTransition {
                    from,
                    attempted: event,
                });
            }
        };
        self.state = next;
        self.reason = reason.into();
        self.entered_at = chrono::Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_disabled_pending_connected() {
        let mut c = McpConnection::new("notion");
        c.transition(StateTransition::Enable, "").unwrap();
        assert_eq!(c.state, McpConnectionState::Pending);
        c.transition(StateTransition::ConnectOk, "").unwrap();
        assert_eq!(c.state, McpConnectionState::Connected);
    }

    #[test]
    fn failed_can_retry_via_enable() {
        let mut c = McpConnection::new("n");
        c.transition(StateTransition::Enable, "").unwrap();
        c.transition(StateTransition::ConnectFail, "tcp reset").unwrap();
        assert_eq!(c.state, McpConnectionState::Failed);
        c.transition(StateTransition::Enable, "retry").unwrap();
        assert_eq!(c.state, McpConnectionState::Pending);
    }

    #[test]
    fn disable_allowed_from_any_state() {
        for initial_state in [
            McpConnectionState::Pending,
            McpConnectionState::Connected,
            McpConnectionState::Failed,
            McpConnectionState::NeedsAuth,
        ] {
            let mut c = McpConnection::new("n");
            c.state = initial_state;
            c.transition(StateTransition::Disable, "operator off").unwrap();
            assert_eq!(c.state, McpConnectionState::Disabled);
        }
    }

    #[test]
    fn illegal_transition_is_rejected_and_reports_event() {
        let mut c = McpConnection::new("n");
        // Disabled → ConnectOk is nonsense.
        let err = c.transition(StateTransition::ConnectOk, "").unwrap_err();
        match err {
            McpCoreError::InvalidTransition { from, attempted } => {
                assert_eq!(from, McpConnectionState::Disabled);
                assert_eq!(attempted, StateTransition::ConnectOk);
            }
            other => panic!("wrong variant: {other:?}"),
        }
        assert_eq!(c.state, McpConnectionState::Disabled);
    }

    #[test]
    fn needs_auth_can_transition_directly_to_connected() {
        let mut c = McpConnection::new("n");
        c.transition(StateTransition::Enable, "").unwrap();
        c.transition(StateTransition::AuthNeeded, "401").unwrap();
        assert_eq!(c.state, McpConnectionState::NeedsAuth);
        c.transition(StateTransition::ConnectOk, "creds ok").unwrap();
        assert_eq!(c.state, McpConnectionState::Connected);
    }

    #[test]
    fn connected_can_reconnect_via_enable() {
        let mut c = McpConnection::new("n");
        c.transition(StateTransition::Enable, "").unwrap();
        c.transition(StateTransition::ConnectOk, "").unwrap();
        assert_eq!(c.state, McpConnectionState::Connected);
        c.transition(StateTransition::Enable, "soft reconnect").unwrap();
        assert_eq!(c.state, McpConnectionState::Pending);
    }

    #[test]
    fn auth_needed_is_retryable() {
        assert!(McpConnectionState::NeedsAuth.is_retryable());
        assert!(McpConnectionState::Failed.is_retryable());
        assert!(McpConnectionState::Disabled.is_retryable());
        assert!(!McpConnectionState::Connected.is_retryable());
        assert!(!McpConnectionState::Pending.is_retryable());
    }

    #[test]
    fn disconnect_drops_connected_to_failed() {
        let mut c = McpConnection::new("n");
        c.transition(StateTransition::Enable, "").unwrap();
        c.transition(StateTransition::ConnectOk, "").unwrap();
        c.transition(StateTransition::Disconnect, "peer closed").unwrap();
        assert_eq!(c.state, McpConnectionState::Failed);
        assert_eq!(c.reason, "peer closed");
    }
}
