//! ACP (Agent Client Protocol) server skeleton.
//!
//! Transport-free — the caller (e.g. `vac acp serve` in `vac_cli`)
//! supplies a pair of JSONL streams (stdio, WebSocket). This module
//! owns:
//!
//! - handshake validation (expect `Hello` first, reject anything else)
//! - protocol-version negotiation
//! - shaped reply on handshake success / failure
//!
//! Actual submit dispatch lives in the driver: after
//! [`AcpServer::handshake`] succeeds, the driver pulls `InboundEvent`s
//! from the session handle, calls into `vac_session_engine`, and
//! pushes resulting `OutboundEvent`s on the outbound channel.

use std::time::Duration;

use tracing::info;

use crate::error::{BridgeError, BridgeResult};
use crate::event::{InboundEvent, OutboundEvent};
use crate::session::{RemoteSession, RemoteSessionHandle, SessionAttachState};

/// Highest protocol version the server understands. Clients at or
/// below this version still connect; higher versions are rejected
/// until we've extended the protocol.
pub const ACP_PROTOCOL_VERSION: u16 = 1;

/// Lowest protocol version the server accepts. A client sending
/// `protocol_version: 0` (e.g. missing field defaulting to zero) is
/// rejected so broken clients fail loudly at handshake time.
pub const ACP_MIN_PROTOCOL_VERSION: u16 = 1;

/// Default wall-clock deadline for the first inbound after the
/// transport connects. A client that attaches but never sends Hello
/// must not wedge a server task.
pub const ACP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Outcome of a successful handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpHandshake {
    pub client: String,
    pub protocol_version: u16,
}

/// Stateless helper that runs the handshake step against a session.
#[derive(Debug, Clone)]
pub struct AcpServer {
    pub server_version: String,
    /// Per-handshake deadline for the first inbound. Defaults to
    /// [`ACP_HANDSHAKE_TIMEOUT`]; tests override to a few millis.
    pub handshake_timeout: Duration,
}

impl AcpServer {
    #[must_use]
    pub fn new(server_version: impl Into<String>) -> Self {
        Self {
            server_version: server_version.into(),
            handshake_timeout: ACP_HANDSHAKE_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_handshake_timeout(mut self, d: Duration) -> Self {
        self.handshake_timeout = d;
        self
    }

    /// Pull one inbound event (bounded by `handshake_timeout`); if it
    /// is a valid `Hello` within the accepted protocol-version band,
    /// send `Welcome` and flip the session state to `Attached`. Any
    /// other inbound in this state is a protocol violation — the
    /// server answers with `Error` and returns the failure.
    ///
    /// Re-entrancy: a session that is already `Attached` or
    /// `Detached` rejects a second handshake with `BridgeError::Protocol`.
    pub async fn handshake(&self, session: &mut RemoteSession) -> BridgeResult<AcpHandshake> {
        let handle = session.handle();
        // Re-entrancy guard.
        match handle.state().await {
            SessionAttachState::Connecting => {}
            SessionAttachState::Attached => {
                return Err(BridgeError::Protocol("handshake already complete".into()));
            }
            SessionAttachState::Detached => {
                return Err(BridgeError::Protocol(
                    "session detached; cannot handshake".into(),
                ));
            }
        }

        let ev = match tokio::time::timeout(self.handshake_timeout, session.next_inbound()).await {
            Ok(Some(ev)) => ev,
            Ok(None) => {
                return Err(BridgeError::Handshake("client hung up before hello".into()));
            }
            Err(_) => {
                return Err(BridgeError::HandshakeTimeout(
                    self.handshake_timeout.as_millis() as u64,
                ));
            }
        };
        let shake = match ev {
            InboundEvent::Hello {
                client,
                protocol_version,
            } => {
                if protocol_version > ACP_PROTOCOL_VERSION
                    || protocol_version < ACP_MIN_PROTOCOL_VERSION
                {
                    let msg = format!(
                        "client protocol {protocol_version} outside server band \
                         [{ACP_MIN_PROTOCOL_VERSION}, {ACP_PROTOCOL_VERSION}]"
                    );
                    // Best-effort notify; a closed client shouldn't
                    // pull us further off the happy path.
                    let _ = handle.outbound_tx.try_send(OutboundEvent::Error {
                        reason: msg.clone(),
                    });
                    return Err(BridgeError::Handshake(msg));
                }
                AcpHandshake {
                    client,
                    protocol_version,
                }
            }
            other => {
                let msg = format!(
                    "expected Hello, got {}",
                    serde_json::to_value(&other)
                        .ok()
                        .and_then(|v| v["kind"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "?".into()),
                );
                let _ = handle.outbound_tx.try_send(OutboundEvent::Error {
                    reason: msg.clone(),
                });
                return Err(BridgeError::Handshake(msg));
            }
        };
        // Welcome before state flip: if the outbound is closed we must
        // surface the failure AND keep the session in Connecting so
        // the driver can retry rather than pretend we're attached.
        send_welcome(&handle, session.session_id, &self.server_version).await?;
        handle.set_state(SessionAttachState::Attached).await;
        info!(
            target: "vac_bridge::acp",
            client = %shake.client,
            version = shake.protocol_version,
            "acp handshake complete",
        );
        Ok(shake)
    }
}

async fn send_welcome(
    handle: &RemoteSessionHandle,
    session_id: uuid::Uuid,
    server_version: &str,
) -> BridgeResult<()> {
    handle
        .outbound_tx
        .send(OutboundEvent::Welcome {
            session_id,
            server_version: server_version.to_string(),
        })
        .await
        .map_err(|_| BridgeError::Other("outbound channel closed during welcome".into()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handshake_accepts_hello_and_sends_welcome() {
        let mut s = RemoteSession::new(4);
        let h = s.handle();
        h.inbound_tx
            .send(InboundEvent::Hello {
                client: "tests".into(),
                protocol_version: 1,
            })
            .await
            .unwrap();
        let srv = AcpServer::new("0.1-test");
        let hs = srv.handshake(&mut s).await.unwrap();
        assert_eq!(hs.client, "tests");
        assert!(h.is_attached().await);
        let welcome = s.next_outbound().await.unwrap();
        assert!(matches!(welcome, OutboundEvent::Welcome { .. }));
    }

    #[tokio::test]
    async fn handshake_rejects_wrong_first_event() {
        let mut s = RemoteSession::new(4);
        let h = s.handle();
        h.inbound_tx
            .send(InboundEvent::Submit {
                text: "premature".into(),
            })
            .await
            .unwrap();
        let srv = AcpServer::new("0.1-test");
        let err = srv.handshake(&mut s).await.unwrap_err();
        assert!(matches!(err, BridgeError::Handshake(_)));
        let out = s.next_outbound().await.unwrap();
        assert!(matches!(out, OutboundEvent::Error { .. }));
    }

    #[tokio::test]
    async fn handshake_rejects_future_protocol_version() {
        let mut s = RemoteSession::new(4);
        let h = s.handle();
        h.inbound_tx
            .send(InboundEvent::Hello {
                client: "too-new".into(),
                protocol_version: ACP_PROTOCOL_VERSION + 9,
            })
            .await
            .unwrap();
        let srv = AcpServer::new("0.1-test");
        let err = srv.handshake(&mut s).await.unwrap_err();
        assert!(matches!(err, BridgeError::Handshake(_)));
    }

    #[tokio::test]
    async fn handshake_rejects_below_min_protocol_version() {
        let mut s = RemoteSession::new(4);
        let h = s.handle();
        h.inbound_tx
            .send(InboundEvent::Hello {
                client: "too-old".into(),
                protocol_version: 0,
            })
            .await
            .unwrap();
        let srv = AcpServer::new("0.1-test");
        let err = srv.handshake(&mut s).await.unwrap_err();
        assert!(matches!(err, BridgeError::Handshake(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn handshake_times_out_when_client_sends_nothing() {
        let mut s = RemoteSession::new(4);
        // Keep the handle alive so the channel isn't closed; just
        // never send a Hello.
        let h = s.handle();
        let srv =
            AcpServer::new("0.1-test").with_handshake_timeout(std::time::Duration::from_millis(50));
        let err = srv.handshake(&mut s).await.unwrap_err();
        assert!(matches!(err, BridgeError::HandshakeTimeout(50)));
        // Session must NOT have flipped to Attached on timeout.
        assert!(!h.is_attached().await);
    }

    #[tokio::test]
    async fn handshake_is_not_reentrant() {
        let mut s = RemoteSession::new(4);
        let h = s.handle();
        h.inbound_tx
            .send(InboundEvent::Hello {
                client: "once".into(),
                protocol_version: 1,
            })
            .await
            .unwrap();
        let srv = AcpServer::new("0.1-test");
        srv.handshake(&mut s).await.unwrap();
        let err = srv.handshake(&mut s).await.unwrap_err();
        assert!(matches!(err, BridgeError::Protocol(_)));
    }

    /// Compile-check: PermissionMediator trait object is Send + Sync
    /// and can be moved into a spawned task. The body never runs;
    /// it exists only to lock in the bounds.
    #[allow(dead_code)]
    fn _mediator_trait_object_bounds() {
        use crate::permission::{PermissionMediator, StaticAllowMediator};
        use std::sync::Arc;
        let m: Arc<dyn PermissionMediator> = Arc::new(StaticAllowMediator::allow());
        let _ = tokio::spawn(async move {
            let _ = &m;
        });
    }
}
