//! F5.7 — End-to-end integration test for the bridge permission
//! round-trip: handshake → engine requests permission → operator
//! responds → decision flows back → engine proceeds.
//!
//! The transport is stubbed with in-memory channels (no stdio /
//! WebSocket). The test exercises the three moving parts that the
//! bridge owns end-to-end:
//!
//! 1. `AcpServer::handshake` walks Hello → Welcome and attaches.
//! 2. A simulated engine pushes `PermissionRequest` out.
//! 3. A simulated client pushes back `PermissionResponse`.
//! 4. The driver resolves the oneshot with the client's verdict and
//!    `await_decision` surfaces the correct value.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;
use vac_bridge::{
    AcpServer, InboundEvent, OutboundEvent, PermissionDecision, PermissionRequest,
    RemoteSession, SessionAttachState,
    permission::await_decision,
};

#[tokio::test]
async fn permission_request_allow_round_trip() {
    let mut session = RemoteSession::new(8);
    let handle = session.handle();

    // Client side: send Hello, then drive a permission allow reply.
    let client_side = {
        let inbound_tx = handle.inbound_tx.clone();
        tokio::spawn(async move {
            inbound_tx
                .send(InboundEvent::Hello {
                    client: "it-client".into(),
                    protocol_version: 1,
                })
                .await
                .unwrap();
            // Client waits for the server's PermissionRequest, then answers.
            // Simulated by a short delay — in real life the client's read
            // loop routes the OutboundEvent to the UI and the operator
            // decides. We then push the answer back.
            tokio::time::sleep(Duration::from_millis(20)).await;
            inbound_tx
                .send(InboundEvent::PermissionResponse {
                    request_id: Uuid::nil(), // stubbed: mapping table below
                    allow: true,
                    reason: None,
                })
                .await
                .unwrap();
        })
    };

    // Server side: handshake first.
    let srv = AcpServer::new("it-server");
    let hs = srv.handshake(&mut session).await.unwrap();
    assert_eq!(hs.client, "it-client");
    assert_eq!(handle.state().await, SessionAttachState::Attached);
    // Drain the Welcome so the outbound buffer is fresh.
    let welcome = session.next_outbound().await.unwrap();
    assert!(matches!(welcome, OutboundEvent::Welcome { .. }));

    // Driver (us): push PermissionRequest outward, wait for the client's
    // response via oneshot. Real drivers keep a HashMap<request_id, tx>;
    // stubbed here with a single-entry map.
    let pending: Arc<Mutex<HashMap<Uuid, oneshot::Sender<PermissionDecision>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let req = PermissionRequest {
        id: Uuid::new_v4(),
        tool: "file_write".into(),
        summary: "overwrite /tmp/x".into(),
        arguments: serde_json::json!({"path": "/tmp/x"}),
        deadline_ms: 500,
    };
    let (tx_decision, rx_decision) = oneshot::channel();
    pending.lock().await.insert(req.id, tx_decision);
    handle
        .outbound_tx
        .send(OutboundEvent::PermissionRequest {
            request_id: req.id,
            tool: req.tool.clone(),
            summary: req.summary.clone(),
        })
        .await
        .unwrap();

    // Pull inbound events from the session until we see the response.
    // In a real driver this runs inside a spawned read loop; the test
    // inlines it for simplicity.
    loop {
        let ev = session.next_inbound().await.expect("client sent reply");
        if let InboundEvent::PermissionResponse {
            request_id: _,
            allow,
            ..
        } = ev
        {
            // In the real driver the router maps request_id -> tx; here
            // there's only one pending request so we just drain it.
            let maybe_tx = pending.lock().await.remove(&req.id);
            if let Some(tx) = maybe_tx {
                let _ = tx.send(if allow {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Deny
                });
            }
            break;
        }
    }

    // Engine side: await the decision with a deadline.
    let decision = await_decision(rx_decision, req.deadline_ms).await.unwrap();
    assert_eq!(decision, PermissionDecision::Allow);

    client_side.await.unwrap();
}

#[tokio::test]
async fn permission_request_deny_round_trip() {
    let mut session = RemoteSession::new(4);
    let handle = session.handle();
    let inbound_tx = handle.inbound_tx.clone();
    inbound_tx
        .send(InboundEvent::Hello {
            client: "deny-client".into(),
            protocol_version: 1,
        })
        .await
        .unwrap();
    AcpServer::new("srv").handshake(&mut session).await.unwrap();
    let _ = session.next_outbound().await; // welcome

    let (tx, rx) = oneshot::channel();
    inbound_tx
        .send(InboundEvent::PermissionResponse {
            request_id: Uuid::nil(),
            allow: false,
            reason: Some("operator rejected".into()),
        })
        .await
        .unwrap();
    // Drain the response and fire the decision.
    if let InboundEvent::PermissionResponse { allow, .. } =
        session.next_inbound().await.unwrap()
    {
        tx.send(if allow {
            PermissionDecision::Allow
        } else {
            PermissionDecision::Deny
        })
        .unwrap();
    }
    let decision = await_decision(rx, 500).await.unwrap();
    assert_eq!(decision, PermissionDecision::Deny);
}
