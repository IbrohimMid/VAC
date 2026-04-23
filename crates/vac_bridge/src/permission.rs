//! Permission mediation — the bridge forwards a permission request
//! to the remote client, waits for a response (with a timeout), and
//! returns the decision to the engine. Drivers implement
//! [`PermissionMediator`] to wire this to their transport.
//!
//! The default [`StaticAllowMediator`] is for tests + `--yolo`-style
//! headless runs; production CLI + TUI drivers should install a
//! mediator that surfaces the request to a real operator.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::error::{BridgeError, BridgeResult};

/// One request to the operator. `summary` is a one-line description
/// suitable for a modal footer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: Uuid,
    pub tool: String,
    pub summary: String,
    /// Optional structured arguments for the client to render.
    #[serde(default)]
    pub arguments: serde_json::Value,
    /// How long the engine is willing to wait before giving up.
    pub deadline_ms: u64,
}

/// What the operator said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecision {
    Allow,
    Deny,
}

#[async_trait]
pub trait PermissionMediator: Send + Sync {
    async fn request(&self, req: PermissionRequest) -> BridgeResult<PermissionDecision>;
}

/// Test/headless mediator: always returns the configured decision.
/// Useful for `vac run --yolo` semantics and engine integration tests.
#[derive(Debug, Clone)]
pub struct StaticAllowMediator {
    pub decision: PermissionDecision,
}

impl StaticAllowMediator {
    #[must_use]
    pub fn allow() -> Self {
        Self {
            decision: PermissionDecision::Allow,
        }
    }

    #[must_use]
    pub fn deny() -> Self {
        Self {
            decision: PermissionDecision::Deny,
        }
    }
}

#[async_trait]
impl PermissionMediator for StaticAllowMediator {
    async fn request(&self, _req: PermissionRequest) -> BridgeResult<PermissionDecision> {
        Ok(self.decision)
    }
}

/// Helper for driver mediators that want to resolve the request via
/// a oneshot channel driven by the transport's inbound read loop.
/// Wraps the channel wait in a tokio timeout so a dead client can't
/// hang the engine.
pub async fn await_decision(
    rx: oneshot::Receiver<PermissionDecision>,
    deadline_ms: u64,
) -> BridgeResult<PermissionDecision> {
    match tokio::time::timeout(Duration::from_millis(deadline_ms), rx).await {
        Ok(Ok(d)) => Ok(d),
        Ok(Err(_)) => Err(BridgeError::Other("permission channel dropped".into())),
        Err(_) => Err(BridgeError::PermissionTimeout(deadline_ms)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> PermissionRequest {
        PermissionRequest {
            id: Uuid::new_v4(),
            tool: "file_write".into(),
            summary: "write /tmp/x".into(),
            arguments: serde_json::Value::Null,
            deadline_ms: 100,
        }
    }

    #[tokio::test]
    async fn static_allow_mediator_returns_allow() {
        let m = StaticAllowMediator::allow();
        assert_eq!(m.request(req()).await.unwrap(), PermissionDecision::Allow);
    }

    #[tokio::test]
    async fn static_deny_mediator_returns_deny() {
        let m = StaticAllowMediator::deny();
        assert_eq!(m.request(req()).await.unwrap(), PermissionDecision::Deny);
    }

    #[tokio::test(start_paused = true)]
    async fn await_decision_honors_timeout() {
        let (_tx, rx) = oneshot::channel::<PermissionDecision>();
        let err = await_decision(rx, 50).await.unwrap_err();
        assert!(matches!(err, BridgeError::PermissionTimeout(50)));
    }

    #[tokio::test]
    async fn await_decision_returns_sent_value() {
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tx.send(PermissionDecision::Allow).unwrap();
        });
        let d = await_decision(rx, 500).await.unwrap();
        assert_eq!(d, PermissionDecision::Allow);
    }

    #[tokio::test]
    async fn await_decision_surfaces_dropped_channel() {
        let (tx, rx) = oneshot::channel::<PermissionDecision>();
        drop(tx);
        let err = await_decision(rx, 500).await.unwrap_err();
        assert!(matches!(err, BridgeError::Other(_)));
    }
}
