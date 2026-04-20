//! Session task helpers — resume, switch, snapshot.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::RuntimeUpdate;
use vac_core::engine::VacEngine;

use crate::{InputEvent, ToolCall};
use super::{ActiveUpdateTx, backend::handle_runtime_update};

pub(super) async fn resume_session_into_tui(
    engine: Arc<Mutex<VacEngine>>,
    active_update_tx: ActiveUpdateTx,
    input_tx: mpsc::Sender<InputEvent>,
    id: String,
) {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(e) => {
            let _ = input_tx
                .send(InputEvent::Error(format!("Invalid session id: {e}")))
                .await;
            return;
        }
    };

    let project_root = {
        let eng = engine.lock().await;
        match eng.status().await {
            Ok(status) => status.project_root,
            Err(e) => {
                let _ = input_tx
                    .send(InputEvent::Error(format!(
                        "Failed to read engine status: {e}"
                    )))
                    .await;
                return;
            }
        }
    };

    let title = match vac_core::session::Session::load(&project_root, uuid) {
        Ok(Some(s)) => format!("Session {}", &s.id.to_string()[..8]),
        _ => format!("Session {}", &id[..id.len().min(8)]),
    };

    let _ = input_tx
        .send(InputEvent::SessionRestored {
            id: id.clone(),
            title,
            messages: vec![],
        })
        .await;

    tokio::spawn(async move {
        let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<RuntimeUpdate>();
        let input_tx_inner = input_tx.clone();
        let stream_uuid = uuid::Uuid::new_v4();

        *active_update_tx.lock().await = Some(update_tx.clone());

        tokio::spawn(async move {
            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
            while let Some(update) = update_rx.recv().await {
                handle_runtime_update(update, &input_tx_inner, stream_uuid, &mut active_tools)
                    .await;
            }
        });

        let result = {
            let mut eng = engine.lock().await;
            eng.resume_run_state(uuid, Some(update_tx), None, None)
                .await
        };

        *active_update_tx.lock().await = None;

        if let Err(e) = result {
            let _ = input_tx
                .send(InputEvent::Error(format!("Failed to resume session: {e}")))
                .await;
        }
    });
}
