//! `vac acp` — start the ACP editor-facing agent server, wired to VacEngine.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn execute(project_root: PathBuf, port: u16) -> anyhow::Result<()> {
    println!("🔌 Starting VAC ACP server on port {port}...");
    println!("   Protocol: acp/1.0 (JSON-over-TCP, newline-delimited)");
    println!("   Sessions: {}/.vac/sessions/", project_root.display());
    println!("🛡️  Permission Mode: PROMPT (Auto-rejecting tools requiring approval over ACP)");
    println!("   Press Ctrl+C to stop.\n");

    // Initialize engine
    let mut engine = vac_core::VacEngine::new(project_root.clone()).await?;
    engine.init().await?;
    let engine = Arc::new(RwLock::new(engine));

    let server = vac_core::AcpServer::new(&project_root);

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

            // Create approval channels for this task
            let (approval_tx, approval_rx) =
                tokio::sync::mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();

            // We drop approval_tx immediately so that if a tool requires approval,
            // vil_swarm receives a closed channel and auto-rejects the tool instead of hanging.
            // This ensures a consistent permission mode (respecting "ask" policy)
            // even when the entrypoint (ACP) cannot prompt the user interactively.
            drop(approval_tx);

            let result = engine
                .write()
                .await
                .run_task_with_approvals(&task, Some(update_tx), None, Some(approval_rx))
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
        .start(port, handler)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    println!("✓ ACP server ready on port {port}");

    tokio::signal::ctrl_c().await?;
    server.stop().await;
    println!("\n✓ ACP server stopped.");
    Ok(())
}
