//! `vac acp` — start the ACP editor-facing agent server, wired to VacEngine.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn execute(project_root: PathBuf, port: u16) -> anyhow::Result<()> {
    println!("🔌 Starting VAC ACP server on port {port}...");
    println!("   Protocol: acp/1.0 (JSON-over-TCP, newline-delimited)");
    println!("   Sessions: {}/.vac/sessions/", project_root.display());
    println!("🛡️  Permission Mode: PROMPT (Approvals resolved via ACP approve_tool)");
    println!("   Press Ctrl+C to stop.\n");

    // Initialize engine
    let mut engine = vac_core::VacEngine::new(project_root.clone()).await?;
    engine.init().await?;
    let approvals = engine.approval_handle();
    let engine = Arc::new(Mutex::new(engine));

    let server = vac_core::AcpServer::new(&project_root);

    let approval_handler: vac_core::acp::ApprovalHandler = Arc::new(move |tool_call_id,
                                                                      approved,
                                                                      feedback| {
        let approvals = approvals.clone();
        Box::pin(async move {
            if approved {
                approvals
                    .approve(tool_call_id)
                    .await
                    .map_err(|e| e.to_string())
            } else {
                approvals
                    .reject(tool_call_id, feedback)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
    });

    // Task handler: delegates to VacEngine, streams RuntimeUpdate as ACP events
    let handler: vac_core::acp::TaskHandler = Arc::new(move |task, _session_id| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = engine.clone();

        tokio::spawn(async move {
            let (update_tx, mut update_rx) =
                tokio::sync::mpsc::unbounded_channel::<vac_core::RuntimeUpdate>();

            // Forward RuntimeUpdate → ACP events
            let tx_fwd = tx.clone();
            tokio::spawn(async move {
                while let Some(update) = update_rx.recv().await {
                    if let Some(event) = vac_core::acp::runtime_update_to_acp_event(&update) {
                        let _ = tx_fwd.send(event);
                    }
                }
            });

            let mut engine = engine.lock().await;
            let result = engine
                .run_task_with_approvals(&task, Some(update_tx), None, None)
                .await;

            match result {
                Ok(r) => {
                    let _ = tx.send(serde_json::json!({
                        "event": "completed",
                        "data": {
                            "summary": r.summary,
                            "modified_files": r.modified_files,
                            "created_files": r.created_files,
                            "validation_score": r.validation_score,
                            "elapsed_ms": r.elapsed_ms,
                            "tokens": r.total_tokens_used
                        }
                    }));
                }
                Err(e) => {
                    let _ = tx.send(serde_json::json!({
                        "event": "failed",
                        "data": { "reason": e.to_string() }
                    }));
                }
            }
        });

        rx
    });

    server
        .start(port, handler, Some(approval_handler))
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    println!("✓ ACP server ready on port {port}");

    tokio::signal::ctrl_c().await?;
    server.stop().await;
    println!("\n✓ ACP server stopped.");
    Ok(())
}
