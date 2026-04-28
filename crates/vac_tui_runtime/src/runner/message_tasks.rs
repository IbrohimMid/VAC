//! Message task helpers — UserMessage, AcceptTool, RejectTool output events.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::RuntimeUpdate;
use vac_core::engine::VacEngine;

use super::{
    ActiveUpdateTx,
    backend::{handle_runtime_update, resolve_tool_approval},
};
use crate::{ContentPart, InputEvent, LoadingOperation, ToolCall};

/// Handle `OutputEvent::UserMessage` — send a user turn to the engine with optional
/// multimodal image parts, wiring runtime updates back into the TUI input channel.
pub(super) async fn handle_user_message(
    project_root: std::path::PathBuf,
    engine: Arc<Mutex<VacEngine>>,
    input_tx: mpsc::Sender<InputEvent>,
    active_update_tx: ActiveUpdateTx,
    msg: String,
    parts: Vec<ContentPart>,
) {
    let _ = input_tx
        .send(InputEvent::StartLoadingOperation(
            LoadingOperation::LlmRequest,
        ))
        .await;

    tokio::spawn(async move {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();
        let input_tx_inner = input_tx.clone();
        let stream_uuid = uuid::Uuid::new_v4();

        // Store active channels for structured approval routing
        *active_update_tx.lock().await = Some(update_tx.clone());

        tokio::spawn(async move {
            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
            while let Some(update) = update_rx.recv().await {
                handle_runtime_update(update, &input_tx_inner, stream_uuid, &mut active_tools)
                    .await;
            }
        });

        // Convert TUI ContentParts to LLM ImageParts for multimodal
        // Generic data-URL parsing: "data:<media_type>;base64,<data>"
        let image_parts: Vec<vil_llm::provider::ImagePart> = parts
            .iter()
            .filter_map(|p| {
                p.image_url.as_ref().and_then(|url| {
                    let raw = &url.url;
                    let after_data = raw.strip_prefix("data:")?;
                    let (meta, data) = after_data.split_once(";base64,")?;
                    Some(vil_llm::provider::ImagePart {
                        source_type: "base64".to_string(),
                        media_type: meta.to_string(),
                        data: data.to_string(),
                    })
                })
            })
            .collect();

        if image_parts.is_empty() {
            let _ = super::engine_adapter::run_via_session_engine(
                project_root,
                engine.clone(),
                &msg,
                update_tx,
            )
            .await;
        } else {
            let mut eng = engine.lock().await;
            let _ = eng
                .run_task_with_images(&msg, Some(update_tx), None, None, image_parts)
                .await;
        }

        // Clear active channels when task completes
        *active_update_tx.lock().await = None;
    });
}

/// Handle `OutputEvent::AcceptTool` — resolve a pending tool approval as accepted.
pub(super) async fn handle_accept_tool(approvals: &vac_approvals::ApprovalHandle, tc: ToolCall) {
    if let Err(e) = resolve_tool_approval(approvals, tc.id.clone(), true, None).await {
        log::error!("Failed to approve tool call {}: {}", tc.id, e);
    }
}

/// Handle `OutputEvent::RejectTool` — resolve a pending tool approval as rejected.
pub(super) async fn handle_reject_tool(
    approvals: &vac_approvals::ApprovalHandle,
    tc: ToolCall,
    reason: Option<String>,
) {
    if let Err(e) = resolve_tool_approval(approvals, tc.id.clone(), false, reason).await {
        log::error!("Failed to reject tool call {}: {}", tc.id, e);
    }
}
