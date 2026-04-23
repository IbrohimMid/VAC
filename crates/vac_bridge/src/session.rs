//! `RemoteSession` — the bridge-side view of a VAC session. Owns the
//! inbound/outbound channels + handshake state. Does not spawn the
//! engine — that's the driver's job (they call into
//! `vac_session_engine::submit_one` with the resulting
//! outbound channel + a permission mediator from this crate).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::event::{InboundEvent, OutboundEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionAttachState {
    /// Transport connected; Hello not yet received.
    Connecting,
    /// Hello accepted; session ready for submits.
    Attached,
    /// Client detached; no further I/O expected.
    Detached,
}

/// The shared handle a driver binds to both sides of the transport.
/// Producers (transport read loop) push `InboundEvent`s via
/// [`RemoteSessionHandle::inbound_tx`]; consumers (engine) pull via
/// [`RemoteSession::next_inbound`].
#[derive(Debug, Clone)]
pub struct RemoteSessionHandle {
    pub session_id: Uuid,
    pub inbound_tx: mpsc::Sender<InboundEvent>,
    pub outbound_tx: mpsc::Sender<OutboundEvent>,
    state: Arc<Mutex<SessionAttachState>>,
}

impl RemoteSessionHandle {
    pub async fn state(&self) -> SessionAttachState {
        *self.state.lock().await
    }

    pub async fn set_state(&self, s: SessionAttachState) {
        *self.state.lock().await = s;
    }

    /// True once the client has sent `Hello`.
    pub async fn is_attached(&self) -> bool {
        self.state().await == SessionAttachState::Attached
    }
}

/// Driver-side owner of the session's receive end.
#[derive(Debug)]
pub struct RemoteSession {
    pub session_id: Uuid,
    inbound_rx: mpsc::Receiver<InboundEvent>,
    outbound_rx: mpsc::Receiver<OutboundEvent>,
    handle: RemoteSessionHandle,
}

impl RemoteSession {
    /// Create a fresh session + handle pair with a given channel
    /// buffer. Buffer bounds both directions to back-pressure slow
    /// consumers — unbounded would let a stuck TUI balloon memory.
    ///
    /// `buffer` must be ≥ 1. `tokio::sync::mpsc::channel(0)` panics
    /// at runtime; buffer=1 can deadlock the handshake's welcome-send
    /// when the outbound reader hasn't started polling yet. Drivers
    /// should pass at least 4.
    #[must_use]
    pub fn new(buffer: usize) -> Self {
        Self::with_id(Uuid::new_v4(), buffer)
    }

    pub fn with_id(session_id: Uuid, buffer: usize) -> Self {
        assert!(buffer >= 1, "RemoteSession buffer must be >= 1");
        let (inbound_tx, inbound_rx) = mpsc::channel(buffer);
        let (outbound_tx, outbound_rx) = mpsc::channel(buffer);
        let state = Arc::new(Mutex::new(SessionAttachState::Connecting));
        let handle = RemoteSessionHandle {
            session_id,
            inbound_tx,
            outbound_tx,
            state,
        };
        Self {
            session_id,
            inbound_rx,
            outbound_rx,
            handle,
        }
    }

    /// Share-safe handle the transport loop should clone + use.
    pub fn handle(&self) -> RemoteSessionHandle {
        self.handle.clone()
    }

    pub async fn next_inbound(&mut self) -> Option<InboundEvent> {
        self.inbound_rx.recv().await
    }

    pub async fn next_outbound(&mut self) -> Option<OutboundEvent> {
        self.outbound_rx.recv().await
    }

    /// Splits the session into its constituent inbound and outbound receivers.
    pub fn split(self) -> (mpsc::Receiver<InboundEvent>, mpsc::Receiver<OutboundEvent>) {
        (self.inbound_rx, self.outbound_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_session_starts_connecting() {
        let s = RemoteSession::new(8);
        assert_eq!(s.handle().state().await, SessionAttachState::Connecting);
    }

    #[tokio::test]
    async fn inbound_and_outbound_are_bidirectional() {
        let mut s = RemoteSession::new(4);
        let h = s.handle();
        h.inbound_tx
            .send(InboundEvent::Hello {
                client: "test".into(),
                protocol_version: 1,
            })
            .await
            .unwrap();
        h.outbound_tx
            .send(OutboundEvent::Welcome {
                session_id: s.session_id,
                server_version: "0.1".into(),
            })
            .await
            .unwrap();
        let ib = s.next_inbound().await.unwrap();
        let ob = s.next_outbound().await.unwrap();
        assert!(matches!(ib, InboundEvent::Hello { .. }));
        assert!(matches!(ob, OutboundEvent::Welcome { .. }));
    }

    #[tokio::test]
    async fn state_transitions_observable() {
        let s = RemoteSession::new(1);
        let h = s.handle();
        h.set_state(SessionAttachState::Attached).await;
        assert!(h.is_attached().await);
        h.set_state(SessionAttachState::Detached).await;
        assert!(!h.is_attached().await);
    }
}
