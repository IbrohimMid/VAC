use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use async_trait::async_trait;
use vac_bridge::{
    AcpServer, InboundEvent, OutboundEvent, PermissionDecision, PermissionMediator,
    PermissionRequest, RemoteSession, StdioPermissionMediator,
};
use vac_session_engine::{
    CompactConfig, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SlashProcessor,
    SubmitContext, TranscriptKind, TranscriptWriter, TrivialCompactBoundary, UsageTracker,
    submit_one,
};

struct MockAdapter {
    mediator: Arc<StdioPermissionMediator>,
}

#[async_trait]
impl LlmAdapter for MockAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        let req = PermissionRequest {
            id: Uuid::new_v4(),
            tool: "mock_tool".into(),
            summary: "needs approval".into(),
            arguments: serde_json::json!({}),
            deadline_ms: 1000,
        };
        let decision = self.mediator.request(req).await;

        let result_content = match decision {
            Ok(PermissionDecision::Allow) => "allowed",
            _ => "denied",
        };

        Ok(LlmResponse {
            provider: "mock".into(),
            model: "mock".into(),
            content: result_content.into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: Vec::new(),
        })
    }
}

#[tokio::test]
async fn e2e_remote_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    let server = AcpServer::new("vac-acp/1.0");
    let mut session = RemoteSession::new(10);
    let handle = session.handle();

    // Simulating the transport read loop pushing to inbound_tx
    let inbound_tx = handle.inbound_tx.clone();
    tokio::spawn(async move {
        // 1. Handshake
        inbound_tx
            .send(InboundEvent::Hello {
                client: "test-client".into(),
                protocol_version: 1,
            })
            .await
            .unwrap();

        // 2. Submit
        // We add a tiny delay to ensure the server processes handshake first
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        inbound_tx
            .send(InboundEvent::Submit {
                text: "do the thing".into(),
            })
            .await
            .unwrap();
    });

    // Handshake server-side
    server.handshake(&mut session).await.unwrap();

    let session_id = session.session_id;
    let (mut inbound_rx, mut outbound_rx) = session.split();

    let pending_permissions = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mediator = Arc::new(StdioPermissionMediator::new(
        handle.outbound_tx.clone(),
        pending_permissions.clone(),
    ));

    let writer = TranscriptWriter::new(root.clone());

    // Server loop
    let handle_for_server = handle.clone();
    let _server_task = tokio::spawn(async move {
        while let Some(event) = inbound_rx.recv().await {
            match event {
                InboundEvent::Submit { text } => {
                    let ctx = SubmitContext::new(session_id, text);

                    let writer = writer.clone();
                    let adapter = MockAdapter {
                        mediator: mediator.clone(),
                    };
                    let handle_outbound = handle_for_server.outbound_tx.clone();

                    tokio::spawn(async move {
                        let slash = SlashProcessor::new();
                        let compact = TrivialCompactBoundary::default();
                        let usage = UsageTracker::new();

                        submit_one(
                            ctx,
                            &writer,
                            &slash,
                            &compact,
                            &usage,
                            &adapter,
                            CompactConfig::default(),
                            None,
                        )
                        .await
                        .unwrap();

                        handle_outbound
                            .send(OutboundEvent::SubmitFinished)
                            .await
                            .unwrap();
                    });
                }
                InboundEvent::PermissionResponse {
                    request_id, allow, ..
                } => {
                    let mut p = pending_permissions.lock().await;
                    if let Some(tx) = p.remove(&request_id) {
                        let decision = if allow {
                            PermissionDecision::Allow
                        } else {
                            PermissionDecision::Deny
                        };
                        let _ = tx.send(decision);
                    }
                }
                InboundEvent::Detach => break,
                _ => {}
            }
        }
    });

    let welcome = outbound_rx.recv().await.unwrap();
    assert!(matches!(welcome, OutboundEvent::Welcome { .. }));

    // The submit_one will call adapter.complete(), which will send a PermissionRequest
    let req = outbound_rx.recv().await.unwrap();
    let req_id = match req {
        OutboundEvent::PermissionRequest { request_id, .. } => request_id,
        _ => panic!("Expected PermissionRequest, got {:?}", req),
    };

    // Client responds
    handle
        .inbound_tx
        .send(InboundEvent::PermissionResponse {
            request_id: req_id,
            allow: true,
            reason: None,
        })
        .await
        .unwrap();

    // Wait for SubmitFinished from outbound
    while let Some(event) = outbound_rx.recv().await {
        if matches!(event, OutboundEvent::SubmitFinished) {
            break;
        }
    }

    // Check transcript
    let writer = TranscriptWriter::new(root.clone());
    let rows = writer.read(session_id).await.unwrap();

    // Accepted, LlmRequest, LlmResponse, Finished
    assert!(rows.iter().any(|r| r.kind == TranscriptKind::Accepted));
    let response_row = rows
        .iter()
        .find(|r| r.kind == TranscriptKind::LlmResponse)
        .unwrap();
    assert_eq!(response_row.content["content"], "allowed");
}
