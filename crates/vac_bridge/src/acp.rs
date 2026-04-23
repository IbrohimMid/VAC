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

use tracing::info;

use crate::error::{BridgeError, BridgeResult};
use crate::event::{InboundEvent, OutboundEvent};
use crate::session::{RemoteSession, RemoteSessionHandle, SessionAttachState};

/// Protocol version this implementation speaks. Clients sending a
/// lower version still connect; higher versions are rejected until
/// we've extended the protocol.
pub const ACP_PROTOCOL_VERSION: u16 = 1;

/// Outcome of a successful handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpHandshake {
    pub client: String,
    pub protocol_version: u16,
}

/// Stateless helper that runs the handshake step against a session.
pub struct AcpServer {
    pub server_version: String,
}

impl AcpServer {
    pub fn new(server_version: impl Into<String>) -> Self {
        Self {
            server_version: server_version.into(),
        }
    }

    /// Pull one inbound event; if it is a valid `Hello`, transition
    /// the session to `Attached` and push a `Welcome` outbound. Any
    /// other inbound in this state is a protocol violation — the
    /// server answers with `Error` and returns the failure.
    pub async fn handshake(
        &self,
        session: &mut RemoteSession,
    ) -> BridgeResult<AcpHandshake> {
        let handle = session.handle();
        let ev = session
            .next_inbound()
            .await
            .ok_or_else(|| BridgeError::Handshake("client hung up before hello".into()))?;
        let shake = match ev {
            InboundEvent::Hello {
                client,
                protocol_version,
            } => {
                if protocol_version > ACP_PROTOCOL_VERSION {
                    let msg = format!(
                        "client protocol {protocol_version} > server max {ACP_PROTOCOL_VERSION}"
                    );
                    let _ = handle
                        .outbound_tx
                        .send(OutboundEvent::Error { reason: msg.clone() })
                        .await;
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
                let _ = handle
                    .outbound_tx
                    .send(OutboundEvent::Error {
                        reason: msg.clone(),
                    })
                    .await;
                return Err(BridgeError::Handshake(msg));
            }
        };
        handle.set_state(SessionAttachState::Attached).await;
        send_welcome(&handle, session.session_id, &self.server_version).await?;
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

}
