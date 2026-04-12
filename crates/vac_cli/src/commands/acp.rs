//! `vac acp` — start the ACP editor-facing agent server.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, port: u16) -> anyhow::Result<()> {
    println!("🔌 Starting VAC ACP server on port {port}...");
    println!("   Protocol: acp/1.0 (JSON-over-TCP, newline-delimited)");
    println!("   Sessions: {}/.vac/sessions/", project_root.display());
    println!("   Press Ctrl+C to stop.\n");

    let server = vac_core::AcpServer::new(&project_root);

    // Minimal task handler — in production this would delegate to VacEngine
    let handler: vac_core::acp::TaskHandler = std::sync::Arc::new(move |task, _session_id| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let _ = tx.send(serde_json::json!({ "event": "status", "data": { "message": format!("Task received: {task}") } }));
        let _ = tx.send(serde_json::json!({ "event": "status", "data": { "message": "Engine not attached in standalone mode" } }));
        rx
    });

    server.start(port, handler).await.map_err(|e| anyhow::anyhow!(e))?;

    // Keep running until Ctrl+C
    tokio::signal::ctrl_c().await?;
    server.stop().await;
    println!("\n✓ ACP server stopped.");
    Ok(())
}
