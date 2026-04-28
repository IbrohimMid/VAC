//! `vac acp` — start the ACP editor-facing agent server, wired to VacEngine.
//!
//! Protocol: acp/1.0 (JSONL over stdio).

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use uuid::Uuid;

use vac_bridge::{
    AcpServer, InboundEvent, OutboundEvent, PermissionDecision, PermissionMediator,
    PermissionRequest, RemoteSession, StdioPermissionMediator,
};
use vac_session_engine::{
    CompactConfig, EngineError, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SlashProcessor,
    SubmitContext, SubmitEvent, TranscriptWriter, TrivialCompactBoundary, UsageTracker, submit_one,
};

pub struct AcpEngineAdapter {
    engine: Arc<Mutex<vac_core::VacEngine>>,
    outbound_tx: mpsc::Sender<OutboundEvent>,
    mediator: Arc<StdioPermissionMediator>,
    approvals: vac_approvals::ApprovalHandle,
}

#[async_trait]
impl LlmAdapter for AcpEngineAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        let (rt_tx, mut rt_rx) = mpsc::unbounded_channel::<vac_core::engine::RuntimeUpdate>();

        let outbound = self.outbound_tx.clone();
        let mediator = self.mediator.clone();
        let approvals = self.approvals.clone();

        let translator = tokio::spawn(async move {
            while let Some(update) = rt_rx.recv().await {
                match update {
                    vac_core::engine::RuntimeUpdate::AssistantChunk(text) => {
                        let _ = outbound.send(OutboundEvent::Chunk { text }).await;
                    }
                    vac_core::engine::RuntimeUpdate::Failed(reason) => {
                        let _ = outbound.send(OutboundEvent::SubmitAborted { reason }).await;
                    }
                    vac_core::engine::RuntimeUpdate::ApprovalRequired {
                        tool_call_id,
                        tool_name,
                        arguments,
                        explanation,
                    } => {
                        let req = PermissionRequest {
                            id: Uuid::new_v4(), // We map the request ID
                            tool: tool_name.clone(),
                            summary: explanation
                                .unwrap_or_else(|| format!("Tool {} requires approval", tool_name)),
                            arguments,
                            deadline_ms: 300000, // 5 minutes
                        };
                        let decision = mediator.request(req).await;
                        if let Ok(PermissionDecision::Allow) = decision {
                            let _ = approvals.approve(tool_call_id).await;
                        } else {
                            let _ = approvals
                                .reject(tool_call_id, Some("Denied by operator".to_string()))
                                .await;
                        }
                    }
                    _ => {}
                }
            }
        });

        let result = {
            let mut engine = self.engine.lock().await;
            engine
                .run_task_with_updates(&req.prompt, Some(rt_tx))
                .await
                .map_err(|e| EngineError::Other(format!("vac_core engine: {e}")))?
        };

        let _ = translator.await;

        Ok(LlmResponse {
            provider: "vac_core".into(),
            model: result
                .agent_contributions
                .first()
                .map(|c| c.agent_role.clone())
                .unwrap_or_else(|| "unknown".into()),
            content: result.summary,
            input_tokens: req.prompt.split_whitespace().count() as u64,
            output_tokens: result.total_tokens_used,
            tool_calls: Vec::new(),
        })
    }
}

pub async fn execute(project_root: PathBuf, _port: u16) -> anyhow::Result<()> {
    // Initialize engine
    let mut engine = vac_core::VacEngine::new(project_root.clone()).await?;
    let _warnings = engine.init().await?;
    let approvals = engine.approval_handle();
    let engine = Arc::new(Mutex::new(engine));

    let server = AcpServer::new("vac-acp/1.0");
    let mut session = RemoteSession::new(100);
    let handle = session.handle();

    let pending_permissions: Arc<
        Mutex<std::collections::HashMap<Uuid, tokio::sync::oneshot::Sender<PermissionDecision>>>,
    > = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mediator = Arc::new(StdioPermissionMediator::new(
        handle.outbound_tx.clone(),
        pending_permissions.clone(),
    ));

    // Spawn stdio reader
    let inbound_tx = handle.inbound_tx.clone();
    let pending_reader = pending_permissions.clone();
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<InboundEvent>(&line) {
                if let InboundEvent::PermissionResponse {
                    request_id, allow, ..
                } = event
                {
                    let mut p = pending_reader.lock().await;
                    if let Some(tx) = p.remove(&request_id) {
                        let decision = if allow {
                            PermissionDecision::Allow
                        } else {
                            PermissionDecision::Deny
                        };
                        let _ = tx.send(decision);
                    }
                    continue;
                }
                if inbound_tx.send(event).await.is_err() {
                    break;
                }
            } else {
                let _ = inbound_tx.send(InboundEvent::Detach).await;
                break;
            }
        }
        let _ = inbound_tx.send(InboundEvent::Detach).await;
    });

    // Handshake
    let hs = server.handshake(&mut session).await?;
    let session_id = session.session_id;

    let writer = TranscriptWriter::new(project_root.clone());
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();

    // Audit C1 fix: ACP host now ships live gate + dispatcher.
    // Pre-fix `submit_one` was called with `CompactConfig::default()`
    // — an ACP peer that asked the local adapter to run a tool
    // fell back to `UnsupportedDispatcher` silently. Wired via the
    // same builders the TUI uses.
    let acp_registry: Arc<vac_tools::ToolRegistry> = Arc::new(vac_tools::ToolRegistry::new());
    vac_tools::builtin::register_builtin_tools(&acp_registry).await?;
    let acp_ctx = Arc::new(
        vac_tools::registry::ToolContext::new(project_root.clone()).with_session_id(session_id),
    );
    // Audit P0.2 closure — ACP host also runs the full live gate
    // stack (PolicyGate + HookGate), matching the TUI / `vac run`
    // path. Policy loaded from `.vac/policy.toml` with unlimited
    // fallback for fresh projects.
    let acp_policy = vac_core::policy_limits::PolicyLimits::load(&project_root)
        .await
        .map_err(|e| anyhow::anyhow!(".vac/policy.toml: {e}"))?;
    let acp_policy_tracker = Arc::new(vac_core::policy_limits::PolicyTracker::new(acp_policy));
    let acp_gate = vac_tui_runtime::runner::dispatcher::build_live_gate_with(
        &project_root,
        Some(acp_policy_tracker),
        None,
    )
    .await?;
    let base_compact_cfg = vac_tui_runtime::runner::dispatcher::live_compact_config(
        acp_registry.clone(),
        acp_ctx.clone(),
        Some(acp_gate),
    );

    let mut stdout = tokio::io::stdout();

    let (mut inbound_rx, mut outbound_rx) = session.split();

    // Event loop
    loop {
        tokio::select! {
            inbound = inbound_rx.recv() => {
                match inbound {
                    Some(event) => match event {
                        InboundEvent::Hello { .. } => {
                            let _ = handle.outbound_tx.send(OutboundEvent::Error {
                                reason: "Protocol violation: Hello after handshake".into(),
                            }).await;
                        }
                        InboundEvent::Submit { text } => {
                            let ctx = SubmitContext::new(session_id, text);

                            let adapter = AcpEngineAdapter {
                                engine: engine.clone(),
                                outbound_tx: handle.outbound_tx.clone(),
                                mediator: mediator.clone(),
                                approvals: approvals.clone(),
                            };

                            let res = submit_one(
                                ctx,
                                &writer,
                                &slash,
                                &compact,
                                &usage,
                                &adapter,
                                base_compact_cfg.clone(),
                                None,
                            ).await;

                            if let Err(e) = res {
                                let _ = handle.outbound_tx.send(OutboundEvent::Error {
                                    reason: e.to_string(),
                                }).await;
                            } else {
                                let _ = handle.outbound_tx.send(OutboundEvent::SubmitFinished).await;
                            }
                        }
                        InboundEvent::Detach => {
                            break;
                        }
                        _ => {}
                    }
                    None => break,
                }
            }
            outbound = outbound_rx.recv() => {
                match outbound {
                    Some(event) => {
                        let mut line = serde_json::to_string(&event).unwrap_or_default();
                        line.push('\n');
                        if stdout.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                        if stdout.flush().await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}
