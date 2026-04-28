//! F5.7 — End-to-end integration test for the bridge permission
//! round-trip: handshake → engine requests permission → client
//! responds → decision flows back.
//!
//! The transport is stubbed with in-memory channels. Coordination
//! between client and server halves is driven by explicit oneshots
//! (not timers) so the test is deterministic — a client task waits
//! on a signal that the permission request has been emitted before
//! sending its response. That mirrors the real protocol: the client's
//! read loop only posts a response after it has seen the request.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;
use vac_bridge::{
    AcpServer, InboundEvent, OutboundEvent, PermissionDecision, RemoteSession, SessionAttachState,
    permission::await_decision,
};

/// Run one round-trip with the configured decision. Returns the
/// resolved decision the engine saw.
async fn run_round_trip(client_allow: bool) -> PermissionDecision {
    let mut session = RemoteSession::new(8);
    let handle = session.handle();

    // ── Handshake ────────────────────────────────────────────────
    handle
        .inbound_tx
        .send(InboundEvent::Hello {
            client: "it-client".into(),
            protocol_version: 1,
        })
        .await
        .unwrap();
    let srv = AcpServer::new("it-server");
    srv.handshake(&mut session).await.unwrap();
    assert_eq!(handle.state().await, SessionAttachState::Attached);
    // Drain the Welcome so subsequent outbound reads see PermissionRequest.
    let welcome = session.next_outbound().await.unwrap();
    assert!(matches!(welcome, OutboundEvent::Welcome { .. }));

    // ── Driver state: pending permissions mapped by request_id ──
    let pending: Arc<Mutex<HashMap<Uuid, oneshot::Sender<PermissionDecision>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let request_id = Uuid::new_v4();
    let (tx_decision, rx_decision) = oneshot::channel();
    pending.lock().await.insert(request_id, tx_decision);

    // Signal fired *after* the engine emits PermissionRequest; the
    // client waits on it so the send order cannot race.
    let (request_seen_tx, request_seen_rx) = oneshot::channel::<()>();

    // Client task: wait until it's signaled that the request was
    // emitted, then post the response carrying the real request_id.
    let client_inbound_tx = handle.inbound_tx.clone();
    let client_side = tokio::spawn(async move {
        request_seen_rx.await.unwrap();
        client_inbound_tx
            .send(InboundEvent::PermissionResponse {
                request_id,
                allow: client_allow,
                reason: None,
            })
            .await
            .unwrap();
    });

    // Engine emits the request.
    handle
        .outbound_tx
        .send(OutboundEvent::PermissionRequest {
            request_id,
            tool: "file_write".into(),
            summary: "overwrite /tmp/x".into(),
        })
        .await
        .unwrap();
    // Signal the client: "request is on the wire, you may respond".
    request_seen_tx.send(()).unwrap();

    // Router: consume inbound events until we find the matching
    // response; route its verdict into the pending oneshot.
    loop {
        let ev = session.next_inbound().await.expect("response arrives");
        if let InboundEvent::PermissionResponse {
            request_id: got_id,
            allow,
            ..
        } = ev
        {
            assert_eq!(got_id, request_id, "request_id must round-trip");
            let tx = pending
                .lock()
                .await
                .remove(&got_id)
                .expect("pending entry matches response");
            tx.send(if allow {
                PermissionDecision::Allow
            } else {
                PermissionDecision::Deny
            })
            .unwrap();
            break;
        }
    }

    client_side.await.unwrap();

    await_decision(rx_decision, 500).await.unwrap()
}

#[tokio::test]
async fn permission_request_allow_round_trip() {
    assert_eq!(run_round_trip(true).await, PermissionDecision::Allow);
}

#[tokio::test]
async fn permission_request_deny_round_trip() {
    assert_eq!(run_round_trip(false).await, PermissionDecision::Deny);
}
