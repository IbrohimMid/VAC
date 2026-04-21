//! VIL task helpers — InvokeVilTool output event.

use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::engine::VacEngine;

use crate::InputEvent;

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
                let formatted = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| result.to_string());
                let content = format!(
                    "**`{}`** result:\n\n```json\n{}\n```",
                    tool_name, formatted
                );
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
