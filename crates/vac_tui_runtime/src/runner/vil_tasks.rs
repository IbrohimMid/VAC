//! VIL task helpers — InvokeVilTool output event + vil dev runner bridge (T14).

use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::engine::VacEngine;

use crate::InputEvent;

/// Spawn a `vil dev` runner and bridge its [`RunnerEvent`]s to the TUI event
/// loop as [`InputEvent::VilDevEvent`]. Silently no-ops if `dev_command` is
/// empty.
pub(crate) fn spawn_vil_dev_bridge(
    dev_command: &str,
    cwd: &Path,
    input_tx: mpsc::Sender<InputEvent>,
) {
    if dev_command.is_empty() {
        return;
    }
    let (tx, mut rx) = mpsc::channel(64);
    let mut runner = crate::services::vil_dev_runner::VilDevRunner::new(tx);
    let cmd = dev_command.to_string();
    let cwd = cwd.to_path_buf();
    tokio::spawn(async move {
        if let Err(e) = runner.spawn(&cmd, &cwd).await {
            let _ = input_tx
                .send(InputEvent::Error(format!("vil dev failed to start: {e}")))
                .await;
            return;
        }
        while let Some(event) = rx.recv().await {
            let _ = input_tx.send(InputEvent::VilDevEvent(event)).await;
        }
    });
}

/// Handle `OutputEvent::InvokeVilTool` — execute a VIL tool directly via the
/// engine and surface formatted JSON output (or an error) as an assistant
/// message. Spawns internally.
pub(super) fn handle_invoke_vil_tool(
    engine: Arc<Mutex<VacEngine>>,
    input_tx: mpsc::Sender<InputEvent>,
    tool_name: String,
    args: serde_json::Value,
) {
    tokio::spawn(async move {
        let eng = engine.lock().await;
        match eng.execute_tool_direct(&tool_name, args).await {
            Ok(result) => {
                let formatted =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                let content = format!("**`{}`** result:\n\n```json\n{}\n```", tool_name, formatted);
                let _ = input_tx.send(InputEvent::AssistantMessage(content)).await;
            }
            Err(e) => {
                let _ = input_tx
                    .send(InputEvent::Error(format!(
                        "Tool '{}' failed: {}",
                        tool_name, e
                    )))
                    .await;
            }
        }
    });
}
